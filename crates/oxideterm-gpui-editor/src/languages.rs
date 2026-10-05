// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use gpui::{
    AnyElement, App, BorrowAppContext, Global, InteractiveElement, IntoElement, MouseButton,
    ParentElement, Styled, div, px, rgb,
};
use oxideterm_editor_syntax::{LanguageId, PluginGrammar, PluginGrammarSource};
use oxideterm_theme::ThemeTokens;
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

/// Shared by native editors and streamed previews that do not own an editor buffer.
pub fn render_language_plugin_notice(
    language: LanguageId,
    tokens: &ThemeTokens,
    cx: &App,
) -> Option<AnyElement> {
    let name = language.plugin_key()?.to_string();
    let plugins = cx.try_global::<EditorLanguagePlugins>()?;
    let failed = plugins
        .grammars
        .get(&language)
        .is_some_and(|grammar| grammar.failed());
    if (plugins.grammars.contains_key(&language) && !failed)
        || plugins.dismissed.contains(&language)
        || plugins.missing_label.is_empty()
    {
        return None;
    }
    let message = if failed {
        format!(
            "{}: {}",
            language.plugin_display_name()?,
            plugins.failed_label
        )
    } else {
        plugins
            .missing_label
            .replace("{{language}}", language.plugin_display_name()?)
    };
    use oxideterm_gpui_ui::button::{ButtonOptions, ButtonSize, ButtonVariant, button_with};
    Some(
        div()
            .debug_selector(|| "editor-language-notice".into())
            .flex()
            .flex_wrap()
            .flex_shrink_0()
            .items_center()
            .gap(px(8.0))
            .px(px(10.0))
            .py(px(4.0))
            .bg(rgb(tokens.ui.bg_panel))
            .text_color(rgb(tokens.ui.text_muted))
            .text_size(px(tokens.metrics.ui_text_xs))
            .child(div().flex_1().min_w(px(0.0)).child(message))
            .child(
                button_with(
                    tokens,
                    plugins.manage_label.clone(),
                    ButtonOptions {
                        variant: ButtonVariant::Ghost,
                        size: ButtonSize::Sm,
                        ..Default::default()
                    },
                )
                .on_mouse_down(MouseButton::Left, move |_, window, cx| {
                    window.dispatch_action(
                        Box::new(ManageLanguagePlugin {
                            language: name.clone(),
                        }),
                        cx,
                    );
                    cx.stop_propagation();
                }),
            )
            .child(
                button_with(
                    tokens,
                    plugins.dismiss_label.clone(),
                    ButtonOptions {
                        variant: ButtonVariant::Ghost,
                        size: ButtonSize::Sm,
                        ..Default::default()
                    },
                )
                .on_mouse_down(MouseButton::Left, move |_, window, cx| {
                    cx.update_global::<EditorLanguagePlugins, _>(|plugins, _| {
                        plugins.dismissed.insert(language);
                    });
                    window.refresh();
                    cx.stop_propagation();
                }),
            )
            .into_any_element(),
    )
}

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
