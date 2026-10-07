// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use gpui::{
    AnyElement, App, BorrowAppContext, Global, InteractiveElement, IntoElement, MouseButton,
    ParentElement, Styled, div, px, rgb,
};
use oxideterm_editor_syntax::{LanguageId, PluginGrammar, PluginGrammarSource};
use oxideterm_plugin_manifest::NativePluginLanguageDefinition;
use oxideterm_theme::ThemeTokens;
use std::path::Path;
use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
};

#[derive(Clone, Debug, PartialEq, gpui::Action, serde::Deserialize)]
#[action(no_json)]
pub struct ManageLanguagePlugin {
    pub plugin_id: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EditorLanguageDefinition {
    pub plugin_id: String,
    pub definition: NativePluginLanguageDefinition,
}

#[derive(Default)]
pub struct EditorLanguagePlugins {
    pub(crate) grammars: HashMap<LanguageId, Arc<PluginGrammar>>,
    pub(crate) definitions: HashMap<LanguageId, EditorLanguageDefinition>,
    pub(crate) dismissed: HashSet<LanguageId>,
    pub(crate) missing_label: String,
    pub(crate) failed_label: String,
    pub(crate) manage_label: String,
    pub(crate) dismiss_label: String,
}

impl Global for EditorLanguagePlugins {}

pub fn detect_language(path: Option<&Path>, source: &str, cx: &App) -> Option<LanguageId> {
    let legacy = LanguageId::detect(path, source);
    let registered = path.and_then(|path| {
        cx.try_global::<EditorLanguagePlugins>()?
            .definitions
            .iter()
            .filter_map(|(id, item)| {
                item.definition
                    .match_path(path)
                    .map(|score| (score, item.plugin_id.as_str(), id))
            })
            .max_by(|left, right| left.0.cmp(&right.0).then_with(|| right.1.cmp(left.1)))
            .map(|(score, _, id)| (score, id.clone()))
    });
    if registered
        .as_ref()
        .is_some_and(|(score, _)| *score == usize::MAX)
    {
        return registered.map(|(_, id)| id);
    }
    // Built-in core grammars remain the default for their existing file types.
    if path
        .and_then(LanguageId::from_path)
        .is_some_and(|id| id.plugin_key().is_none())
    {
        return legacy;
    }
    registered.map(|(_, id)| id).or(legacy)
}

/// Shared by native editors and streamed previews that do not own an editor buffer.
pub fn render_language_plugin_notice(
    language: LanguageId,
    tokens: &ThemeTokens,
    cx: &App,
) -> Option<AnyElement> {
    let key = language.plugin_key()?;
    let plugins = cx.try_global::<EditorLanguagePlugins>()?;
    let definition = plugins.definitions.get(&language);
    let name = definition
        .map(|entry| entry.plugin_id.clone())
        .unwrap_or_else(|| format!("com.oxideterm.language.{key}"));
    let display_name = definition
        .and_then(|entry| entry.definition.display_name.as_deref())
        .or_else(|| language.plugin_display_name())?;
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
        format!("{}: {}", display_name, plugins.failed_label)
    } else {
        plugins.missing_label.replace("{{language}}", display_name)
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
                            plugin_id: name.clone(),
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
                        plugins.dismissed.insert(language.clone());
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
        let definitions = cx
            .try_global::<Self>()
            .map(|state| state.definitions.values().cloned().collect())
            .unwrap_or_default();
        Self::update_languages(sources, definitions, cx);
    }

    pub fn update_languages(
        sources: Vec<PluginGrammarSource>,
        definitions: Vec<EditorLanguageDefinition>,
        cx: &mut App,
    ) {
        let previous = cx.try_global::<Self>();
        let mut registered = HashMap::new();
        for item in definitions {
            if item.definition.validate().is_err() {
                continue;
            }
            if let Some(id) = LanguageId::from_plugin_key(&item.definition.id) {
                // Installed definitions precede catalog hints for the same language.
                registered.entry(id).or_insert(item);
            }
        }
        let mut grammars = HashMap::new();
        for source in sources {
            let grammar = previous
                .and_then(|state| state.grammars.get(&source.language))
                .filter(|grammar| grammar.source() == &source && !grammar.failed())
                .cloned()
                .unwrap_or_else(|| Arc::new(PluginGrammar::new(source)));
            grammars
                .entry(grammar.source().language.clone())
                .or_insert(grammar);
        }
        if previous.is_some_and(|state| {
            state.definitions == registered
                && state.grammars.len() == grammars.len()
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
            definitions: registered,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[gpui::test]
    fn catalog_associations_choose_specific_matches_and_preserve_core_defaults(
        cx: &mut gpui::TestAppContext,
    ) {
        let definition = |id: &str, extensions: &[&str], names: &[&str]| EditorLanguageDefinition {
            plugin_id: format!("com.example.{id}"),
            definition: NativePluginLanguageDefinition {
                id: id.into(),
                extensions: extensions.iter().map(|s| (*s).into()).collect(),
                file_names: names.iter().map(|s| (*s).into()).collect(),
                ..Default::default()
            },
        };
        cx.update(|cx| {
            EditorLanguagePlugins::update_languages(
                Vec::new(),
                vec![
                    definition("z-lang", &["expr", "json"], &[]),
                    definition("a-lang", &["expr", "custom.expr"], &["Customfile"]),
                ],
                cx,
            );
            for (path, expected) in [
                ("file.expr", "a-lang"),
                ("file.CUSTOM.EXPR", "a-lang"),
                ("Customfile", "a-lang"),
            ] {
                assert_eq!(
                    detect_language(Some(Path::new(path)), "", cx),
                    LanguageId::from_plugin_key(expected)
                );
            }
            assert_eq!(
                detect_language(Some(Path::new("file.json")), "", cx),
                Some(LanguageId::Json)
            );
            assert_eq!(
                detect_language(Some(Path::new("file.cpp")), "", cx),
                Some(LanguageId::Cpp)
            );
            assert_eq!(
                detect_language(Some(Path::new("unknown.txt")), "", cx),
                None
            );
            assert_eq!(
                detect_language(Some(Path::new("file.expr")), "#!/usr/bin/env python3\n", cx),
                LanguageId::from_plugin_key("a-lang")
            );
            assert_eq!(
                detect_language(None, "#!/usr/bin/env python3\n", cx),
                Some(LanguageId::Python)
            );
        });
    }
}
