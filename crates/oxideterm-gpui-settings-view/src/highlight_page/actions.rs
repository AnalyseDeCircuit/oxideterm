use gpui::Context;
use oxideterm_connections::ConnectionStore;
use oxideterm_i18n::I18n;
use oxideterm_settings::{
    HighlightRule, MAX_HIGHLIGHT_RULE_SETS, MAX_HIGHLIGHT_RULES, PersistedSettings,
    TerminalSemanticScheme, create_default_highlight_rule, create_highlight_rule_set,
    reindex_highlight_rules,
};
use oxideterm_settings_model::{
    add_custom_semantic_rule, create_custom_semantic_scheme, delete_custom_semantic_rule,
    delete_custom_semantic_scheme, edit_custom_semantic_scheme,
};

use crate::entity::SettingsWorkspaceEntity;

use super::{HighlightFileKind, HighlightFileOperation, HighlightRequest};

#[derive(Clone, Copy)]
pub enum HighlightModal {
    OpenRule(usize),
    OpenSemantic,
    CloseRule,
    CloseSemantic,
}

pub enum HighlightEdit {
    NewRule,
    Preset(Vec<HighlightRule>),
    Rule(usize, Box<dyn FnOnce(&mut HighlightRule)>),
    Remove(usize),
    Move(usize, usize),
    CreateSet(String),
    DeleteSet(String),
    CreateSemantic(String),
    DeleteSemantic(String),
    AddSemanticRule,
    DeleteSemanticRule(usize),
    ToggleSemanticRule(usize),
    Priority(usize, i16),
}

pub enum HighlightOutcome {
    None,
    Edit(HighlightEdit),
    Modal(HighlightModal),
    Transfer(HighlightFileOperation),
    Warning(String),
}

impl HighlightEdit {
    pub fn apply(self, settings: &mut PersistedSettings) -> Result<Option<HighlightModal>, String> {
        match self {
            Self::CreateSemantic(name) => {
                create_custom_semantic_scheme(settings, name, TerminalSemanticScheme::Balanced)?;
                return Ok(Some(HighlightModal::OpenSemantic));
            }
            Self::DeleteSemantic(id) => {
                delete_custom_semantic_scheme(settings, &id);
                return Ok(Some(HighlightModal::CloseSemantic));
            }
            Self::CreateSet(name) => {
                let sets = &mut settings.terminal.highlight_rule_sets;
                if sets.len() < MAX_HIGHLIGHT_RULE_SETS {
                    let set = create_highlight_rule_set(name, Vec::new());
                    settings.terminal.default_highlight_rule_set = Some(set.id.clone());
                    sets.push(set);
                }
                return Ok(Some(HighlightModal::CloseRule));
            }
            Self::DeleteSet(id) => {
                settings
                    .terminal
                    .highlight_rule_sets
                    .retain(|set| set.id != id);
                settings.terminal.default_highlight_rule_set = None;
                return Ok(Some(HighlightModal::CloseRule));
            }
            Self::AddSemanticRule => {
                let _ = add_custom_semantic_rule(settings);
            }
            Self::DeleteSemanticRule(index) => {
                delete_custom_semantic_rule(settings, index);
            }
            Self::ToggleSemanticRule(index) => {
                let _ = edit_custom_semantic_scheme(settings, |scheme| {
                    if let Some(rule) = scheme.rules.get_mut(index) {
                        rule.enabled = !rule.enabled;
                    }
                });
            }
            Self::Priority(index, delta) => {
                let _ = edit_custom_semantic_scheme(settings, |scheme| {
                    if let Some(rule) = scheme.rules.get_mut(index) {
                        rule.priority = (i16::from(rule.priority) + delta).clamp(0, 255) as u8;
                    }
                });
            }
            Self::NewRule => {
                let rules = settings.terminal.effective_highlight_rules_mut();
                if rules.len() < MAX_HIGHLIGHT_RULES {
                    let index = rules.len();
                    rules.push(create_default_highlight_rule(|_| {}));
                    *rules = reindex_highlight_rules(std::mem::take(rules));
                    return Ok(Some(HighlightModal::OpenRule(index)));
                }
            }
            Self::Preset(preset) => {
                let rules = settings.terminal.effective_highlight_rules_mut();
                rules.extend(preset);
                *rules = reindex_highlight_rules(std::mem::take(rules));
                rules.truncate(MAX_HIGHLIGHT_RULES);
            }
            Self::Rule(index, edit) => {
                let rules = settings.terminal.effective_highlight_rules_mut();
                if let Some(rule) = rules.get_mut(index) {
                    edit(rule);
                }
                *rules = reindex_highlight_rules(std::mem::take(rules));
            }
            Self::Remove(index) => {
                let rules = settings.terminal.effective_highlight_rules_mut();
                if index < rules.len() {
                    rules.remove(index);
                }
                *rules = reindex_highlight_rules(std::mem::take(rules));
            }
            Self::Move(from, to) => {
                let rules = settings.terminal.effective_highlight_rules_mut();
                if from < rules.len() && to < rules.len() {
                    rules.swap(from, to);
                }
                *rules = reindex_highlight_rules(std::mem::take(rules));
            }
        }
        Ok(None)
    }
}

impl SettingsWorkspaceEntity {
    pub fn highlight_request(
        &mut self,
        request: HighlightRequest,
        settings: &PersistedSettings,
        connections: &ConnectionStore,
        i18n: &I18n,
        cx: &mut Context<Self>,
    ) -> HighlightOutcome {
        use HighlightRequest::*;
        let terminal = &settings.terminal;
        let edit = match request {
            OpenSemantic => return HighlightOutcome::Modal(HighlightModal::OpenSemantic),
            CloseSemantic => return HighlightOutcome::Modal(HighlightModal::CloseSemantic),
            OpenRule(index) => return HighlightOutcome::Modal(HighlightModal::OpenRule(index)),
            CloseRule => return HighlightOutcome::Modal(HighlightModal::CloseRule),
            NewRule => HighlightEdit::NewRule,
            CreateSemantic => HighlightEdit::CreateSemantic(
                i18n.t("settings_view.terminal.highlight_rules.semantic_scheme_default_name"),
            ),
            CreateRuleSet => HighlightEdit::CreateSet(
                i18n.t("settings_view.terminal.highlight_rules.rule_set_default_name"),
            ),
            ImportSemantic | ImportRuleSet | ExportSemantic | ExportRuleSet => {
                let kind = if matches!(request, ImportSemantic | ExportSemantic) {
                    HighlightFileKind::SemanticScheme
                } else {
                    HighlightFileKind::RuleSet
                };
                return if matches!(request, ImportSemantic | ImportRuleSet) {
                    HighlightOutcome::Transfer(HighlightFileOperation::Import(kind))
                } else {
                    HighlightFileOperation::export(kind, settings)
                        .map_or(HighlightOutcome::None, HighlightOutcome::Transfer)
                };
            }
            DeleteRuleSet | DeleteSemantic => {
                let (selected, label, semantic) = if matches!(request, DeleteSemantic) {
                    (
                        terminal.semantic_custom_scheme.as_ref(),
                        "settings_view.terminal.highlight_rules.semantic_scheme_delete_in_use",
                        true,
                    )
                } else {
                    (
                        terminal.default_highlight_rule_set.as_ref(),
                        "settings_view.terminal.highlight_rules.rule_set_delete_in_use",
                        false,
                    )
                };
                let Some(id) = selected else {
                    return HighlightOutcome::None;
                };
                let owner = connections
                    .connections()
                    .iter()
                    .find(|connection| {
                        let terminal = &connection.options.terminal;
                        (if semantic {
                            terminal.semantic_scheme.as_ref()
                        } else {
                            terminal.highlight_rule_set.as_ref()
                        }) == Some(id)
                    })
                    .map(|connection| connection.name.as_str())
                    .or_else(|| {
                        connections
                            .telnet_profiles()
                            .iter()
                            .find(|profile| {
                                let terminal = &profile.terminal;
                                (if semantic {
                                    terminal.semantic_scheme.as_ref()
                                } else {
                                    terminal.highlight_rule_set.as_ref()
                                }) == Some(id)
                            })
                            .map(|profile| profile.name.as_str())
                    });
                if let Some(owner) = owner {
                    return HighlightOutcome::Warning(i18n.t(label).replace("{{name}}", owner));
                }
                if semantic {
                    HighlightEdit::DeleteSemantic(id.clone())
                } else {
                    HighlightEdit::DeleteSet(id.clone())
                }
            }
            AddSemanticRule => HighlightEdit::AddSemanticRule,
            ToggleSemanticRule(index) => HighlightEdit::ToggleSemanticRule(index),
            DeleteSemanticRule(index) => HighlightEdit::DeleteSemanticRule(index),
            Priority(index, delta) => HighlightEdit::Priority(index, delta),
            Move(index, direction) => {
                let to = if direction < 0 {
                    index.checked_sub(1)
                } else {
                    index.checked_add(1)
                };
                let len = terminal.effective_highlight_rules().len();
                let Some(to) = to.filter(|to| index < len && *to < len) else {
                    return HighlightOutcome::None;
                };
                let current = &mut self.highlight.highlight_rule_editor_index;
                if *current == Some(index) {
                    *current = Some(to);
                    cx.notify();
                } else if *current == Some(to) {
                    *current = Some(index);
                    cx.notify();
                }
                HighlightEdit::Move(index, to)
            }
            Remove(index) => {
                if let Some(open) = self.highlight.highlight_rule_editor_index
                    && open > index
                {
                    self.highlight.highlight_rule_editor_index = Some(open - 1);
                    cx.notify();
                }
                HighlightEdit::Remove(index)
            }
        };
        HighlightOutcome::Edit(edit)
    }
}
