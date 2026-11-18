use std::path::PathBuf;

use gpui::{App, Context};
use oxideterm_i18n::I18n;
use oxideterm_settings::{
    HighlightRuleSet, MAX_HIGHLIGHT_RULE_SETS, PersistedSettings, create_highlight_rule_set,
    sanitize_highlight_rule_sets,
};
use oxideterm_settings_model::{
    CUSTOM_SEMANTIC_SCHEME_PREFIX, export_custom_semantic_scheme,
    import_custom_semantic_scheme_named,
};

use crate::entity::SettingsWorkspaceEntity;

#[derive(Clone, Copy)]
pub enum HighlightFileKind {
    RuleSet,
    SemanticScheme,
}

pub enum HighlightFileOperation {
    Import(HighlightFileKind),
    Export {
        kind: HighlightFileKind,
        file_name: String,
        json: String,
    },
}

pub enum HighlightFileResult {
    RuleSet(HighlightRuleSet),
    SemanticScheme { json: String, name: String },
    Exported,
}

impl HighlightFileResult {
    pub fn apply(self, settings: &mut PersistedSettings) -> Result<bool, String> {
        match self {
            Self::RuleSet(set) => {
                if settings.terminal.highlight_rule_sets.len() >= MAX_HIGHLIGHT_RULE_SETS {
                    return Ok(false);
                }
                if let Some(set) = sanitize_highlight_rule_sets(vec![set]).pop() {
                    let set = create_highlight_rule_set(set.name, set.rules);
                    settings.terminal.default_highlight_rule_set = Some(set.id.clone());
                    settings.terminal.highlight_rule_sets.push(set);
                    return Ok(true);
                }
                Ok(false)
            }
            Self::SemanticScheme { json, name } => {
                import_custom_semantic_scheme_named(settings, &json, &name)?;
                Ok(true)
            }
            Self::Exported => Ok(true),
        }
    }
}

impl HighlightFileOperation {
    pub fn export(kind: HighlightFileKind, settings: &PersistedSettings) -> Option<Self> {
        let (name, suffix, json) = match kind {
            HighlightFileKind::RuleSet => {
                let id = settings.terminal.default_highlight_rule_set.as_deref()?;
                let set = settings.terminal.highlight_rule_set(id)?;
                (
                    set.name.as_str(),
                    ".oxideterm-highlights.json",
                    serde_json::to_string_pretty(set).ok()?,
                )
            }
            HighlightFileKind::SemanticScheme => {
                let id = settings.terminal.semantic_custom_scheme.as_deref()?;
                (
                    id.trim_start_matches(CUSTOM_SEMANTIC_SCHEME_PREFIX),
                    ".oxideterm-scheme.json",
                    export_custom_semantic_scheme(settings, id).ok()?,
                )
            }
        };
        let mut stem: String = name
            .chars()
            .map(|c| {
                if c.is_ascii_alphanumeric() || c == '-' {
                    c
                } else {
                    '-'
                }
            })
            .collect();
        if matches!(kind, HighlightFileKind::RuleSet) && stem.trim_matches('-').is_empty() {
            stem = "highlight-rule-set".to_string();
        }
        Some(Self::Export {
            kind,
            file_name: format!("{stem}{suffix}"),
            json,
        })
    }

    pub fn picks_directory(&self) -> bool {
        matches!(self, Self::Export { .. })
    }

    pub fn prompt(&self, i18n: &I18n) -> String {
        i18n.t(match self {
            Self::Import(HighlightFileKind::RuleSet) => {
                "settings_view.terminal.highlight_rules.rule_set_import"
            }
            Self::Import(HighlightFileKind::SemanticScheme) => {
                "settings_view.terminal.highlight_rules.semantic_scheme_import"
            }
            Self::Export {
                kind: HighlightFileKind::RuleSet,
                ..
            } => "settings_view.terminal.highlight_rules.rule_set_export",
            Self::Export {
                kind: HighlightFileKind::SemanticScheme,
                ..
            } => "settings_view.terminal.highlight_rules.semantic_scheme_export",
        })
    }

    fn success(&self, i18n: &I18n) -> String {
        i18n.t(match self {
            Self::Import(HighlightFileKind::RuleSet) => {
                "settings_view.terminal.highlight_rules.rule_set_import_success"
            }
            Self::Import(HighlightFileKind::SemanticScheme) => {
                "settings_view.terminal.highlight_rules.semantic_scheme_import_success"
            }
            Self::Export {
                kind: HighlightFileKind::RuleSet,
                ..
            } => "settings_view.terminal.highlight_rules.rule_set_export_success",
            Self::Export {
                kind: HighlightFileKind::SemanticScheme,
                ..
            } => "settings_view.terminal.highlight_rules.semantic_scheme_export_success",
        })
    }

    fn execute(self, path: PathBuf) -> Result<HighlightFileResult, String> {
        match self {
            Self::Import(HighlightFileKind::RuleSet) => {
                let json = std::fs::read_to_string(path).map_err(|error| error.to_string())?;
                serde_json::from_str(&json)
                    .map(HighlightFileResult::RuleSet)
                    .map_err(|error| error.to_string())
            }
            Self::Import(HighlightFileKind::SemanticScheme) => {
                let stem = path
                    .file_stem()
                    .and_then(|name| name.to_str())
                    .unwrap_or("Imported Scheme");
                let name = if stem.eq_ignore_ascii_case("scheme") {
                    path.parent()
                        .and_then(|path| path.file_name())
                        .and_then(|name| name.to_str())
                        .unwrap_or(stem)
                } else {
                    stem
                }
                .to_owned();
                let json = std::fs::read_to_string(path).map_err(|error| error.to_string())?;
                Ok(HighlightFileResult::SemanticScheme { json, name })
            }
            Self::Export {
                file_name, json, ..
            } => {
                std::fs::write(path.join(file_name), json).map_err(|error| error.to_string())?;
                Ok(HighlightFileResult::Exported)
            }
        }
    }

    pub fn start(
        self,
        pick_path: impl Future<Output = Option<PathBuf>> + 'static,
        runtime: tokio::runtime::Handle,
        i18n: &I18n,
        finish: impl FnOnce(Result<(HighlightFileResult, String), String>, &mut App) + 'static,
        cx: &mut Context<SettingsWorkspaceEntity>,
    ) {
        let success = self.success(i18n);
        cx.spawn(async move |owner, cx| {
            let Some(path) = pick_path.await else {
                return;
            };
            if owner.upgrade().is_none() {
                return;
            }
            let result = runtime
                .spawn_blocking(move || self.execute(path))
                .await
                .map_err(|error| error.to_string())
                .and_then(|result| result);
            let _ = cx.update(move |cx| {
                if owner.upgrade().is_some() {
                    finish(result.map(|result| (result, success)), cx);
                }
            });
        })
        .detach();
    }
}
