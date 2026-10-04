// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use gpui::{App, BorrowAppContext, Global};
use oxideterm_editor_syntax::{LanguageId, PluginGrammar, PluginGrammarSource};
use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
};

#[derive(Clone, Debug, PartialEq, gpui::Action, serde::Deserialize)]
#[action(no_json)]
pub struct ManageLanguagePlugin {
    pub language: String,
}

#[derive(Default)]
pub struct EditorLanguagePlugins {
    pub(crate) grammars: HashMap<LanguageId, Arc<PluginGrammar>>,
    pub(crate) dismissed: HashSet<LanguageId>,
    pub(crate) missing_label: String,
    pub(crate) failed_label: String,
    pub(crate) manage_label: String,
    pub(crate) dismiss_label: String,
}

impl Global for EditorLanguagePlugins {}

impl EditorLanguagePlugins {
    pub fn update(sources: Vec<PluginGrammarSource>, cx: &mut App) {
        let previous = cx.try_global::<Self>();
        let mut grammars = HashMap::new();
        for source in sources {
            let grammar = previous
                .and_then(|state| state.grammars.get(&source.language))
                .filter(|grammar| grammar.source() == &source && !grammar.failed())
                .cloned()
                .unwrap_or_else(|| Arc::new(PluginGrammar::new(source)));
            grammars.insert(grammar.source().language, grammar);
        }
        if previous.is_some_and(|state| {
            state.grammars.len() == grammars.len()
                && grammars.iter().all(|(id, grammar)| {
                    state
                        .grammars
                        .get(id)
                        .is_some_and(|old| Arc::ptr_eq(old, grammar))
                })
        }) {
            return;
        }
        let next = Self {
            grammars,
            dismissed: previous
                .map(|state| state.dismissed.clone())
                .unwrap_or_default(),
            missing_label: previous
                .map(|state| state.missing_label.clone())
                .unwrap_or_default(),
            failed_label: previous
                .map(|state| state.failed_label.clone())
                .unwrap_or_default(),
            manage_label: previous
                .map(|state| state.manage_label.clone())
                .unwrap_or_default(),
            dismiss_label: previous
                .map(|state| state.dismiss_label.clone())
                .unwrap_or_default(),
        };
        cx.set_global(next);
    }

    pub fn set_labels(
        missing: String,
        failed: String,
        manage: String,
        dismiss: String,
        cx: &mut App,
    ) {
        cx.update_default_global::<Self, _>(|state, _cx| {
            state.missing_label = missing;
            state.failed_label = failed;
            state.manage_label = manage;
            state.dismiss_label = dismiss;
        });
    }
}
