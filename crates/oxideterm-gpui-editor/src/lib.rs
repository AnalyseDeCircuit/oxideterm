// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

//! GPUI text editor surface for OxideTerm's native IDE path.
//!
//! This crate deliberately starts with virtualized plain-text rendering only.
//! Syntax, project ownership, and remote save semantics belong to later crates
//! in `docs/native-editor-ide-plan.md`.

mod languages;
mod metrics;
mod settings;
mod surface;
mod viewport;

#[cfg(test)]
mod grammar_fixture {
    use oxideterm_editor_syntax::{
        LanguageId, PluginGrammar, PluginGrammarSource, SyntaxError, SyntaxSession, SyntaxWork,
    };
    include!("../../oxideterm-editor-syntax/tests/support/grammars.rs");

    pub(crate) fn install_rust(cx: &mut gpui::App) {
        crate::EditorLanguagePlugins::update(vec![grammar(LanguageId::Rust).source().clone()], cx);
    }
}

pub use languages::{EditorLanguagePlugins, ManageLanguagePlugin};
pub use metrics::{EditorAppearance, EditorMetrics};
pub use settings::EditorSettings;
pub use surface::{
    EditorCommand, EditorContextMenuLabels, EditorKeybindings, EditorPresentation,
    EditorSaveStatus, EditorScrollAnchor, EditorScrollOrigin, EditorShortcut,
    EditorViewportChanged, SaveCallback, TextEditorView,
};
pub use viewport::{EditorViewport, VisibleRows};
