// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use std::{collections::HashSet, sync::Arc};

use regex::{Regex, RegexBuilder, RegexSet, RegexSetBuilder};

use crate::{
    LocalProcessSpec, TerminalTrigger, TerminalTriggerAction, TerminalTriggerError,
    TerminalTriggerMatchMode, TerminalTriggerScope, TerminalTriggersSnapshot,
    model::TERMINAL_TRIGGERS_SCHEMA_VERSION, template::template_variables,
};

pub const MAX_TRIGGERS: usize = 128;
pub const MAX_TRIGGER_ID_BYTES: usize = 128;
pub const MAX_TRIGGER_NAME_BYTES: usize = 160;
pub const MAX_TRIGGER_DESCRIPTION_BYTES: usize = 1_024;
pub const MAX_PATTERN_BYTES: usize = 4_096;
pub const MAX_ACTION_FIELD_BYTES: usize = 8_192;
pub const MAX_PROCESS_ARGUMENTS: usize = 64;
pub const MAX_CONNECTION_IDS: usize = 256;
pub const MAX_DELAY_MS: u64 = 24 * 60 * 60 * 1_000;
pub const MIN_COOLDOWN_MS: u64 = 100;
pub const MAX_COOLDOWN_MS: u64 = 24 * 60 * 60 * 1_000;
const MAX_COMPILED_REGEX_BYTES: usize = 2 * 1024 * 1024;

pub(crate) struct CompiledTrigger {
    pub(crate) id: String,
    pub(crate) matcher: Regex,
    pub(crate) capture_names: Vec<String>,
    pub(crate) whole_word: bool,
    pub(crate) dispatch: crate::TerminalTriggerDispatch,
    pub(crate) delay_ms: u64,
    pub(crate) cooldown_ms: u64,
}

/// An immutable, generation-tagged set that is safe to share across sessions.
pub struct CompiledTriggerSet {
    pub(crate) generation: u64,
    pub(crate) candidates: RegexSet,
    pub(crate) triggers: Vec<CompiledTrigger>,
}

impl CompiledTriggerSet {
    pub fn compile(
        snapshot: &TerminalTriggersSnapshot,
        generation: u64,
    ) -> Result<Self, TerminalTriggerError> {
        validate_snapshot(snapshot)?;

        let mut triggers = Vec::new();
        let mut candidate_patterns = Vec::new();
        for trigger in snapshot.triggers.iter().filter(|trigger| trigger.enabled) {
            let (matcher, candidate_pattern) = compile_matcher(trigger)?;
            let capture_names = matcher
                .capture_names()
                .flatten()
                .map(str::to_owned)
                .collect();
            triggers.push(CompiledTrigger {
                id: trigger.id.clone(),
                matcher,
                capture_names,
                whole_word: trigger.matcher.whole_word,
                dispatch: trigger.timing.dispatch,
                delay_ms: trigger.timing.delay_ms,
                cooldown_ms: trigger.timing.cooldown_ms,
            });
            candidate_patterns.push(candidate_pattern);
        }
        let candidates = RegexSetBuilder::new(candidate_patterns)
            .size_limit(MAX_COMPILED_REGEX_BYTES)
            .build()
            .map_err(|_| TerminalTriggerError::InvalidRegex)?;
        Ok(Self {
            generation,
            candidates,
            triggers,
        })
    }

    pub fn generation(&self) -> u64 {
        self.generation
    }

    pub fn len(&self) -> usize {
        self.triggers.len()
    }

    pub fn is_empty(&self) -> bool {
        self.triggers.is_empty()
    }
}

/// Compiles enabled rules and returns `None` for the terminal hot-path fast case.
pub fn compile_active(
    snapshot: &TerminalTriggersSnapshot,
    generation: u64,
) -> Result<Option<Arc<CompiledTriggerSet>>, TerminalTriggerError> {
    let compiled = CompiledTriggerSet::compile(snapshot, generation)?;
    Ok((!compiled.is_empty()).then(|| Arc::new(compiled)))
}

pub fn validate_snapshot(snapshot: &TerminalTriggersSnapshot) -> Result<(), TerminalTriggerError> {
    if snapshot.version != TERMINAL_TRIGGERS_SCHEMA_VERSION {
        return Err(TerminalTriggerError::UnsupportedSchema(snapshot.version));
    }
    ensure_collection_len("triggers", snapshot.triggers.len(), MAX_TRIGGERS)?;
    let mut trigger_ids = HashSet::with_capacity(snapshot.triggers.len());
    for trigger in &snapshot.triggers {
        validate_trigger(trigger)?;
        if !trigger_ids.insert(trigger.id.as_str()) {
            return Err(TerminalTriggerError::DuplicateId);
        }
    }
    Ok(())
}

fn validate_trigger(trigger: &TerminalTrigger) -> Result<(), TerminalTriggerError> {
    ensure_required("trigger.id", &trigger.id, MAX_TRIGGER_ID_BYTES)?;
    ensure_required("trigger.name", &trigger.name, MAX_TRIGGER_NAME_BYTES)?;
    ensure_optional(
        "trigger.description",
        trigger.description.as_deref(),
        MAX_TRIGGER_DESCRIPTION_BYTES,
    )?;
    ensure_required(
        "trigger.match.pattern",
        &trigger.matcher.pattern,
        MAX_PATTERN_BYTES,
    )?;
    if trigger.timing.delay_ms > MAX_DELAY_MS {
        return Err(TerminalTriggerError::DelayTooLong);
    }
    if !(MIN_COOLDOWN_MS..=MAX_COOLDOWN_MS).contains(&trigger.timing.cooldown_ms) {
        return Err(TerminalTriggerError::InvalidCooldown);
    }

    let (matcher, _) = compile_matcher(trigger)?;
    if matcher.is_match("") {
        return Err(TerminalTriggerError::EmptyRegexMatch);
    }
    let mut captures = matcher
        .capture_names()
        .flatten()
        .map(str::to_owned)
        .collect::<HashSet<_>>();
    captures.insert("match".to_string());
    validate_action(&trigger.action, &captures)?;
    validate_scope(&trigger.scope)
}

fn compile_matcher(trigger: &TerminalTrigger) -> Result<(Regex, String), TerminalTriggerError> {
    let pattern = match trigger.matcher.mode {
        TerminalTriggerMatchMode::Literal => regex::escape(&trigger.matcher.pattern),
        TerminalTriggerMatchMode::Regex => trigger.matcher.pattern.clone(),
    };
    let matcher = RegexBuilder::new(&pattern)
        .case_insensitive(!trigger.matcher.case_sensitive)
        .size_limit(MAX_COMPILED_REGEX_BYTES)
        .build()
        .map_err(|_| TerminalTriggerError::InvalidRegex)?;
    let candidate_pattern = if trigger.matcher.case_sensitive {
        format!("(?:{pattern})")
    } else {
        format!("(?i:{pattern})")
    };
    Ok((matcher, candidate_pattern))
}

fn validate_action(
    action: &TerminalTriggerAction,
    captures: &HashSet<String>,
) -> Result<(), TerminalTriggerError> {
    match action {
        TerminalTriggerAction::SendText { text, .. } => {
            validate_template_field("action.text", text, captures)
        }
        TerminalTriggerAction::RunQuickCommand { quick_command_id } => ensure_required(
            "action.quickCommandId",
            quick_command_id,
            MAX_TRIGGER_ID_BYTES,
        ),
        TerminalTriggerAction::LaunchLocalProcess { process } => {
            validate_process(process, captures)
        }
    }
}

fn validate_process(
    process: &LocalProcessSpec,
    captures: &HashSet<String>,
) -> Result<(), TerminalTriggerError> {
    let (executable_field, executable, arguments, working_directory) = match process {
        LocalProcessSpec::DirectProgram {
            executable,
            arguments,
            working_directory,
        } => (
            "action.process.executable",
            executable,
            arguments,
            working_directory,
        ),
        LocalProcessSpec::ExplicitShell {
            shell_executable,
            arguments,
            working_directory,
        } => (
            "action.process.shellExecutable",
            shell_executable,
            arguments,
            working_directory,
        ),
    };
    ensure_static_required(executable_field, executable, MAX_ACTION_FIELD_BYTES)?;
    ensure_collection_len(
        "action.process.arguments",
        arguments.len(),
        MAX_PROCESS_ARGUMENTS,
    )?;
    for argument in arguments {
        validate_template_field("action.process.argument", argument, captures)?;
    }
    ensure_static_optional(
        "action.process.workingDirectory",
        working_directory.as_deref(),
        MAX_ACTION_FIELD_BYTES,
    )?;
    Ok(())
}

fn validate_scope(scope: &TerminalTriggerScope) -> Result<(), TerminalTriggerError> {
    let TerminalTriggerScope::SavedConnections { connections } = scope else {
        return Ok(());
    };
    ensure_collection_len("scope.connections", connections.len(), MAX_CONNECTION_IDS)?;
    let mut unique_ids = HashSet::with_capacity(connections.len());
    for connection in connections {
        ensure_required("scope.connectionId", &connection.id, MAX_TRIGGER_ID_BYTES)?;
        if !unique_ids.insert((&connection.kind, &connection.id)) {
            return Err(TerminalTriggerError::DuplicateId);
        }
    }
    Ok(())
}

fn validate_template_field(
    field: &'static str,
    template: &str,
    captures: &HashSet<String>,
) -> Result<(), TerminalTriggerError> {
    ensure_required(field, template, MAX_ACTION_FIELD_BYTES)?;
    for variable in template_variables(template)? {
        if !captures.contains(variable) {
            return Err(TerminalTriggerError::UnknownCapture(variable.to_string()));
        }
    }
    Ok(())
}

fn ensure_required(
    field: &'static str,
    value: &str,
    limit: usize,
) -> Result<(), TerminalTriggerError> {
    if value.trim().is_empty() {
        return Err(TerminalTriggerError::EmptyField { field });
    }
    if value.len() > limit {
        return Err(TerminalTriggerError::FieldTooLong { field, limit });
    }
    Ok(())
}

fn ensure_optional(
    field: &'static str,
    value: Option<&str>,
    limit: usize,
) -> Result<(), TerminalTriggerError> {
    if value.is_some_and(|value| value.len() > limit) {
        return Err(TerminalTriggerError::FieldTooLong { field, limit });
    }
    Ok(())
}

fn ensure_static_required(
    field: &'static str,
    value: &str,
    limit: usize,
) -> Result<(), TerminalTriggerError> {
    ensure_required(field, value, limit)?;
    if !template_variables(value)?.is_empty() {
        return Err(TerminalTriggerError::InvalidTemplate);
    }
    Ok(())
}

fn ensure_static_optional(
    field: &'static str,
    value: Option<&str>,
    limit: usize,
) -> Result<(), TerminalTriggerError> {
    ensure_optional(field, value, limit)?;
    if value.is_some_and(|value| {
        template_variables(value)
            .map(|variables| !variables.is_empty())
            .unwrap_or(true)
    }) {
        return Err(TerminalTriggerError::InvalidTemplate);
    }
    Ok(())
}

fn ensure_collection_len(
    field: &'static str,
    len: usize,
    limit: usize,
) -> Result<(), TerminalTriggerError> {
    if len > limit {
        return Err(TerminalTriggerError::CollectionTooLarge { field, limit });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        LocalProcessSpec, TerminalTriggerAction, TerminalTriggerDispatch, TerminalTriggerMatch,
        TerminalTriggerScope, TerminalTriggerTiming,
    };

    fn trigger(pattern: &str, mode: TerminalTriggerMatchMode) -> TerminalTrigger {
        TerminalTrigger {
            id: "trigger-1".to_string(),
            name: "Test".to_string(),
            description: None,
            enabled: true,
            matcher: TerminalTriggerMatch {
                pattern: pattern.to_string(),
                mode,
                case_sensitive: true,
                whole_word: false,
            },
            action: TerminalTriggerAction::SendText {
                text: "ack ${match}".to_string(),
                append_enter: true,
            },
            timing: TerminalTriggerTiming {
                dispatch: TerminalTriggerDispatch::Immediate,
                delay_ms: 0,
                cooldown_ms: MIN_COOLDOWN_MS,
            },
            scope: TerminalTriggerScope::AllTerminals,
            created_at: 1,
            updated_at: 1,
        }
    }

    fn snapshot(trigger: TerminalTrigger) -> TerminalTriggersSnapshot {
        TerminalTriggersSnapshot {
            version: TERMINAL_TRIGGERS_SCHEMA_VERSION,
            triggers: vec![trigger],
            updated_at: 1,
        }
    }

    #[test]
    fn compiles_literal_metacharacters_as_text() {
        let snapshot = snapshot(trigger("[ready]", TerminalTriggerMatchMode::Literal));
        let compiled = CompiledTriggerSet::compile(&snapshot, 7).unwrap();

        assert!(compiled.triggers[0].matcher.is_match("[ready]"));
        assert!(!compiled.triggers[0].matcher.is_match("r"));
        assert_eq!(compiled.generation(), 7);
    }

    #[test]
    fn validates_named_capture_templates() {
        let mut rule = trigger(
            r"host=(?P<host>[a-z0-9.-]+)",
            TerminalTriggerMatchMode::Regex,
        );
        rule.action = TerminalTriggerAction::SendText {
            text: "ping ${host}".to_string(),
            append_enter: true,
        };

        assert!(validate_snapshot(&snapshot(rule)).is_ok());
    }

    #[test]
    fn rejects_unknown_capture_without_echoing_pattern() {
        let mut rule = trigger("(?P<secret>.+)", TerminalTriggerMatchMode::Regex);
        rule.action = TerminalTriggerAction::SendText {
            text: "${missing}".to_string(),
            append_enter: false,
        };

        let error = validate_snapshot(&snapshot(rule)).unwrap_err();
        assert!(matches!(error, TerminalTriggerError::UnknownCapture(_)));

        let invalid = trigger("(?P<secret>", TerminalTriggerMatchMode::Regex);
        let message = validate_snapshot(&snapshot(invalid))
            .unwrap_err()
            .to_string();
        assert_eq!(message, "terminal trigger regular expression is invalid");
        assert!(!message.contains("secret"));
    }

    #[test]
    fn rejects_empty_matching_regex_and_duplicate_ids() {
        let empty = trigger(".*", TerminalTriggerMatchMode::Regex);
        assert!(matches!(
            validate_snapshot(&snapshot(empty)),
            Err(TerminalTriggerError::EmptyRegexMatch)
        ));

        let rule = trigger("ready", TerminalTriggerMatchMode::Literal);
        let duplicate = TerminalTriggersSnapshot {
            version: TERMINAL_TRIGGERS_SCHEMA_VERSION,
            triggers: vec![rule.clone(), rule],
            updated_at: 1,
        };
        assert!(matches!(
            validate_snapshot(&duplicate),
            Err(TerminalTriggerError::DuplicateId)
        ));
    }

    #[test]
    fn only_process_arguments_accept_capture_templates() {
        let mut rule = trigger(r"user=(?P<user>\w+)", TerminalTriggerMatchMode::Regex);
        rule.action = TerminalTriggerAction::LaunchLocalProcess {
            process: LocalProcessSpec::DirectProgram {
                executable: "/usr/bin/printf".to_string(),
                arguments: vec!["%s".to_string(), "${user}".to_string()],
                working_directory: Some("/tmp".to_string()),
            },
        };
        assert!(validate_snapshot(&snapshot(rule.clone())).is_ok());

        let TerminalTriggerAction::LaunchLocalProcess { process } = &mut rule.action else {
            unreachable!();
        };
        let LocalProcessSpec::DirectProgram { executable, .. } = process else {
            unreachable!();
        };
        *executable = "${user}".to_string();
        assert!(matches!(
            validate_snapshot(&snapshot(rule)),
            Err(TerminalTriggerError::InvalidTemplate)
        ));
    }

    #[test]
    fn disabled_rules_are_validated_but_compile_to_empty_fast_path() {
        let mut rule = trigger("ready", TerminalTriggerMatchMode::Literal);
        rule.enabled = false;
        let snapshot = snapshot(rule);

        assert!(compile_active(&snapshot, 1).unwrap().is_none());
    }
}
