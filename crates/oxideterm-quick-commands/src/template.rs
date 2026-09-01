// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use std::collections::{BTreeMap, HashMap};
use zeroize::Zeroizing;

use crate::{
    QuickCommand, QuickCommandConfirmationPolicy, QuickCommandParameter, QuickCommandParameterKind,
    QuickCommandRisk, QuickCommandTargetProtocol, classify_command_risk,
    quick_command_available_for_target,
};

pub const MAX_QUICK_COMMAND_ARGUMENTS: usize = 32;
pub const MAX_QUICK_COMMAND_ARGUMENT_NAME_BYTES: usize = 64;
pub const MAX_QUICK_COMMAND_ARGUMENT_VALUE_BYTES: usize = 8 * 1024;
pub const MAX_QUICK_COMMAND_EXPANDED_BYTES: usize = 64 * 1024;

#[derive(Clone, Default, Eq, PartialEq)]
pub struct QuickCommandContextValues {
    pub host: Option<Zeroizing<String>>,
    pub username: Option<Zeroizing<String>>,
    pub port: Option<u16>,
    pub cwd: Option<Zeroizing<String>>,
    pub connection: Option<Zeroizing<String>>,
    pub group: Option<Zeroizing<String>>,
    pub selection: Option<Zeroizing<String>>,
}

#[derive(Clone, Eq, PartialEq)]
pub struct QuickCommandTargetContext {
    pub target_id: String,
    pub label: String,
    pub protocol: QuickCommandTargetProtocol,
    pub values: QuickCommandContextValues,
}

#[derive(Clone, Eq, PartialEq)]
pub struct PreparedQuickCommandTarget {
    pub target_id: String,
    pub label: String,
    pub command: Zeroizing<String>,
    pub risk: Option<QuickCommandRisk>,
}

#[derive(Clone, Eq, PartialEq)]
pub struct PreparedQuickCommand {
    pub command_id: String,
    pub targets: Vec<PreparedQuickCommandTarget>,
    pub unavailable_targets: Vec<String>,
    pub confirmation_required: bool,
}

#[derive(Clone, Eq, PartialEq)]
pub enum QuickCommandTemplateError {
    UnterminatedToken,
    UnknownToken(String),
    UnknownModifier(String),
    UnknownParameter(String),
    TooManyParameterValues,
    ParameterValueTooLong(String),
    ExpandedCommandTooLong { target: String },
    MissingParameter(String),
    InvalidChoice { parameter: String },
    MissingContext { target: String, field: String },
}

impl std::fmt::Display for QuickCommandTemplateError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnterminatedToken => formatter.write_str("unterminated template token"),
            Self::UnknownToken(_) => formatter.write_str("unknown template token"),
            Self::UnknownModifier(modifier) => {
                write!(formatter, "unknown template modifier {modifier}")
            }
            Self::UnknownParameter(parameter) => {
                write!(formatter, "unknown parameter {parameter}")
            }
            Self::TooManyParameterValues => formatter.write_str("too many parameter values"),
            Self::ParameterValueTooLong(parameter) => {
                write!(formatter, "value for parameter {parameter} is too long")
            }
            Self::ExpandedCommandTooLong { target } => {
                write!(
                    formatter,
                    "expanded command for target {target} is too long"
                )
            }
            Self::MissingParameter(parameter) => {
                write!(formatter, "missing required parameter {parameter}")
            }
            Self::InvalidChoice { parameter } => {
                write!(formatter, "invalid value for parameter {parameter}")
            }
            Self::MissingContext { target, field } => {
                write!(formatter, "target {target} has no {field} context")
            }
        }
    }
}

impl std::fmt::Debug for QuickCommandTemplateError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Errors retain only structural names; never expose substituted values
        // through panic, log, or diagnostic formatting.
        std::fmt::Display::fmt(self, formatter)
    }
}

pub fn prepare_quick_command(
    command: &QuickCommand,
    targets: &[QuickCommandTargetContext],
    parameter_values: &BTreeMap<String, Zeroizing<String>>,
) -> Result<PreparedQuickCommand, Vec<QuickCommandTemplateError>> {
    let parameters = command
        .parameters
        .iter()
        .map(|parameter| (parameter.name.as_str(), parameter))
        .collect::<HashMap<_, _>>();
    let mut errors = Vec::new();
    validate_parameter_values(&parameters, parameter_values, &mut errors);

    let mut prepared_targets = Vec::new();
    let mut unavailable_targets = Vec::new();
    for target in targets {
        let target_fields = quick_command_target_match_fields(target);
        if !quick_command_available_for_target(command, target.protocol, &target_fields) {
            unavailable_targets.push(target.label.clone());
            continue;
        }
        match resolve_template(&command.command, &parameters, parameter_values, target) {
            Ok(resolved) => prepared_targets.push(PreparedQuickCommandTarget {
                target_id: target.target_id.clone(),
                label: target.label.clone(),
                risk: classify_command_risk(&resolved),
                command: resolved,
            }),
            Err(mut target_errors) => errors.append(&mut target_errors),
        }
    }
    if !errors.is_empty() {
        return Err(errors);
    }
    let confirmation_required = command.confirmation == QuickCommandConfirmationPolicy::Always
        || prepared_targets.iter().any(|target| target.risk.is_some());
    Ok(PreparedQuickCommand {
        command_id: command.id.clone(),
        targets: prepared_targets,
        unavailable_targets,
        confirmation_required,
    })
}

pub fn quick_command_target_match_fields(target: &QuickCommandTargetContext) -> Vec<String> {
    // Availability and execution must derive identity from the same target context.
    let mut fields = vec![target.label.clone()];
    if let Some(host) = target.values.host.as_deref() {
        fields.push(host.to_string());
    }
    if let Some(username) = target.values.username.as_deref() {
        fields.push(username.to_string());
        if let Some(host) = target.values.host.as_deref() {
            fields.push(format!("{username}@{host}"));
        }
    }
    if let Some(connection) = target.values.connection.as_deref() {
        fields.push(connection.to_string());
    }
    fields.retain(|field| !field.trim().is_empty());
    fields.dedup();
    fields
}

pub fn quick_command_can_run_non_interactively(command: &QuickCommand) -> bool {
    // Trigger actions cannot prompt for missing values, so required parameters need defaults.
    validate_quick_command_template(&command.command, &command.parameters).is_ok()
        && command.parameters.iter().all(|parameter| {
            !parameter.required
                || parameter
                    .default_value
                    .as_deref()
                    .is_some_and(|value| !value.trim().is_empty())
        })
}

pub fn quick_command_has_runtime_substitutions(template: &str) -> bool {
    let mut cursor = 0;
    while cursor < template.len() {
        let remainder = &template[cursor..];
        let Some(token_offset) = remainder.find("{{") else {
            return false;
        };
        let token_start = cursor + token_offset;
        if template[..token_start].ends_with('\\') {
            cursor = token_start + 2;
            continue;
        }
        cursor = token_start + 2;
        let Some(token_end) = template[cursor..].find("}}") else {
            return false;
        };
        let token = template[cursor..cursor + token_end].trim();
        if token.starts_with("param.") || token.starts_with("ctx.") {
            return true;
        }
        cursor += token_end + 2;
    }
    false
}

pub fn validate_quick_command_template(
    template: &str,
    command_parameters: &[QuickCommandParameter],
) -> Result<(), Vec<QuickCommandTemplateError>> {
    let parameters = command_parameters
        .iter()
        .map(|parameter| (parameter.name.as_str(), parameter))
        .collect::<HashMap<_, _>>();
    let target = QuickCommandTargetContext {
        target_id: "validation".to_string(),
        label: "validation".to_string(),
        protocol: QuickCommandTargetProtocol::Local,
        // Known context fields use empty values so validation checks token shape, not runtime data.
        values: QuickCommandContextValues {
            host: Some(Zeroizing::new(String::new())),
            username: Some(Zeroizing::new(String::new())),
            port: Some(0),
            cwd: Some(Zeroizing::new(String::new())),
            connection: Some(Zeroizing::new(String::new())),
            group: Some(Zeroizing::new(String::new())),
            selection: Some(Zeroizing::new(String::new())),
        },
    };
    resolve_template(template, &parameters, &BTreeMap::new(), &target).map(|_| ())
}

fn validate_parameter_values(
    parameters: &HashMap<&str, &QuickCommandParameter>,
    parameter_values: &BTreeMap<String, Zeroizing<String>>,
    errors: &mut Vec<QuickCommandTemplateError>,
) {
    for parameter_name in parameter_values.keys() {
        if !parameters.contains_key(parameter_name.as_str()) {
            errors.push(QuickCommandTemplateError::UnknownParameter(
                parameter_name.clone(),
            ));
        }
    }
    if parameter_values.len() > MAX_QUICK_COMMAND_ARGUMENTS {
        errors.push(QuickCommandTemplateError::TooManyParameterValues);
    }
    for (parameter_name, value) in parameter_values {
        if parameter_name.len() > MAX_QUICK_COMMAND_ARGUMENT_NAME_BYTES
            || value.len() > MAX_QUICK_COMMAND_ARGUMENT_VALUE_BYTES
        {
            errors.push(QuickCommandTemplateError::ParameterValueTooLong(
                parameter_name.clone(),
            ));
        }
    }
    for parameter in parameters.values() {
        let value = parameter_values
            .get(&parameter.name)
            .filter(|value| !value.is_empty())
            .map(|value| value.as_str())
            .or(parameter.default_value.as_deref());
        if parameter.required && value.is_none() {
            errors.push(QuickCommandTemplateError::MissingParameter(
                parameter.name.clone(),
            ));
        }
        if parameter.kind == QuickCommandParameterKind::Choice
            && let Some(value) = value
            && !parameter.choices.iter().any(|choice| choice == value)
        {
            errors.push(QuickCommandTemplateError::InvalidChoice {
                parameter: parameter.name.clone(),
            });
        }
    }
}

fn resolve_template(
    template: &str,
    parameters: &HashMap<&str, &QuickCommandParameter>,
    parameter_values: &BTreeMap<String, Zeroizing<String>>,
    target: &QuickCommandTargetContext,
) -> Result<Zeroizing<String>, Vec<QuickCommandTemplateError>> {
    let mut resolved = Zeroizing::new(String::with_capacity(template.len()));
    let mut errors = Vec::new();
    let mut cursor = 0;
    while cursor < template.len() {
        let remainder = &template[cursor..];
        if remainder.starts_with("\\{{") {
            push_expanded(&mut resolved, "{{", target)?;
            cursor += 3;
            continue;
        }
        let Some(token_offset) = remainder.find("{{") else {
            push_expanded(&mut resolved, remainder, target)?;
            break;
        };
        push_expanded(&mut resolved, &remainder[..token_offset], target)?;
        cursor += token_offset + 2;
        let Some(token_end) = template[cursor..].find("}}") else {
            let unfinished = &template[cursor..];
            if quick_command_token_namespace(unfinished.trim_start()) {
                errors.push(QuickCommandTemplateError::UnterminatedToken);
            } else {
                // Double braces belong to many command syntaxes; preserve them unless they
                // explicitly opt into an OxideTerm parameter or context namespace.
                push_expanded(&mut resolved, "{{", target)?;
                push_expanded(&mut resolved, unfinished, target)?;
            }
            break;
        };
        let raw_token = &template[cursor..cursor + token_end];
        let token = raw_token.trim();
        cursor += token_end + 2;
        if !quick_command_token_namespace(token) {
            push_expanded(&mut resolved, "{{", target)?;
            push_expanded(&mut resolved, raw_token, target)?;
            push_expanded(&mut resolved, "}}", target)?;
            continue;
        }
        match resolve_token(token, parameters, parameter_values, target) {
            Ok(value) => push_expanded(&mut resolved, &value, target)?,
            Err(error) => errors.push(error),
        }
    }
    if errors.is_empty() {
        Ok(resolved)
    } else {
        Err(errors)
    }
}

fn quick_command_token_namespace(token: &str) -> bool {
    token.starts_with("param.") || token.starts_with("ctx.")
}

fn push_expanded(
    resolved: &mut Zeroizing<String>,
    value: &str,
    target: &QuickCommandTargetContext,
) -> Result<(), Vec<QuickCommandTemplateError>> {
    if resolved.len().saturating_add(value.len()) > MAX_QUICK_COMMAND_EXPANDED_BYTES {
        return Err(vec![QuickCommandTemplateError::ExpandedCommandTooLong {
            target: target.label.clone(),
        }]);
    }
    resolved.push_str(value);
    Ok(())
}

fn resolve_token(
    token: &str,
    parameters: &HashMap<&str, &QuickCommandParameter>,
    parameter_values: &BTreeMap<String, Zeroizing<String>>,
    target: &QuickCommandTargetContext,
) -> Result<Zeroizing<String>, QuickCommandTemplateError> {
    let (token, modifier) = token
        .split_once('|')
        .map_or((token, None), |(token, modifier)| {
            (token.trim(), Some(modifier.trim()))
        });
    let value = if let Some(name) = token.strip_prefix("param.") {
        let Some(parameter) = parameters.get(name) else {
            return Err(QuickCommandTemplateError::UnknownParameter(
                name.to_string(),
            ));
        };
        parameter_values
            .get(name)
            .filter(|value| !value.is_empty())
            .cloned()
            .or_else(|| parameter.default_value.clone().map(Zeroizing::new))
            .unwrap_or_else(|| Zeroizing::new(String::new()))
    } else if let Some(field) = token.strip_prefix("ctx.") {
        context_value(&target.values, field).ok_or_else(|| {
            QuickCommandTemplateError::MissingContext {
                target: target.label.clone(),
                field: field.to_string(),
            }
        })?
    } else {
        return Err(QuickCommandTemplateError::UnknownToken(token.to_string()));
    };
    match modifier {
        None => Ok(value),
        Some("sh") => Ok(quote_posix_shell_word(&value)),
        Some(modifier) => Err(QuickCommandTemplateError::UnknownModifier(
            modifier.to_string(),
        )),
    }
}

fn quote_posix_shell_word(value: &str) -> Zeroizing<String> {
    let mut quoted = Zeroizing::new(String::with_capacity(value.len().saturating_add(2)));
    quoted.push('\'');
    for character in value.chars() {
        if character == '\'' {
            quoted.push_str("'\"'\"'");
        } else {
            quoted.push(character);
        }
    }
    quoted.push('\'');
    quoted
}

fn context_value(context: &QuickCommandContextValues, field: &str) -> Option<Zeroizing<String>> {
    match field {
        "host" => context.host.clone(),
        "username" => context.username.clone(),
        "port" => context.port.map(|port| Zeroizing::new(port.to_string())),
        "cwd" => context.cwd.clone(),
        "connection" => context.connection.clone(),
        "group" => context.group.clone(),
        "selection" => context.selection.clone(),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::QuickCommandAvailability;

    #[test]
    fn parameters_resolve_once_while_context_resolves_per_target() {
        let command = QuickCommand {
            id: "deploy".to_string(),
            name: "Deploy".to_string(),
            command: "deploy {{param.service}} --host {{ctx.host}}".to_string(),
            category: "ops".to_string(),
            description: None,
            parameters: vec![QuickCommandParameter {
                name: "service".to_string(),
                label: "Service".to_string(),
                required: true,
                ..QuickCommandParameter::default()
            }],
            availability: QuickCommandAvailability::default(),
            confirmation: QuickCommandConfirmationPolicy::Inherit,
            sort_order: 0,
            created_at: 0,
            updated_at: 0,
        };
        let targets = ["a.example.com", "b.example.com"].map(|host| QuickCommandTargetContext {
            target_id: host.to_string(),
            label: host.to_string(),
            protocol: QuickCommandTargetProtocol::Ssh,
            values: QuickCommandContextValues {
                host: Some(Zeroizing::new(host.to_string())),
                ..QuickCommandContextValues::default()
            },
        });
        let values = BTreeMap::from([("service".to_string(), Zeroizing::new("api".to_string()))]);

        let prepared = prepare_quick_command(&command, &targets, &values).unwrap();

        assert_eq!(
            prepared.targets[0].command.as_str(),
            "deploy api --host a.example.com"
        );
        assert_eq!(
            prepared.targets[1].command.as_str(),
            "deploy api --host b.example.com"
        );
    }

    #[test]
    fn template_validation_rejects_unknown_parameters_before_execution() {
        let parameters = vec![QuickCommandParameter {
            name: "service".to_string(),
            label: "Service".to_string(),
            ..QuickCommandParameter::default()
        }];

        let errors = validate_quick_command_template(
            "deploy {{param.sevrice}} --host {{ctx.host}}",
            &parameters,
        )
        .unwrap_err();

        assert!(matches!(
            errors.as_slice(),
            [QuickCommandTemplateError::UnknownParameter(parameter)] if parameter == "sevrice"
        ));
    }

    #[test]
    fn non_oxideterm_double_braces_remain_literal_command_text() {
        let command = QuickCommand {
            id: "docker-log".to_string(),
            name: "Docker log".to_string(),
            command: "docker inspect --format='{{.LogPath}}' container".to_string(),
            category: "custom".to_string(),
            description: None,
            parameters: Vec::new(),
            availability: QuickCommandAvailability::default(),
            confirmation: QuickCommandConfirmationPolicy::Inherit,
            sort_order: 0,
            created_at: 1,
            updated_at: 1,
        };
        let target = QuickCommandTargetContext {
            target_id: "local".to_string(),
            label: "Local".to_string(),
            protocol: QuickCommandTargetProtocol::Local,
            values: QuickCommandContextValues::default(),
        };

        let prepared = prepare_quick_command(&command, &[target], &BTreeMap::new()).unwrap();

        assert_eq!(prepared.targets[0].command.as_str(), command.command);
    }

    #[test]
    fn sh_modifier_quotes_one_posix_shell_word() {
        let command = QuickCommand {
            id: "show".to_string(),
            name: "Show".to_string(),
            command: "printf '%s\\n' {{param.value|sh}}".to_string(),
            category: "custom".to_string(),
            description: None,
            parameters: vec![QuickCommandParameter {
                name: "value".to_string(),
                label: "Value".to_string(),
                required: true,
                ..QuickCommandParameter::default()
            }],
            availability: QuickCommandAvailability::default(),
            confirmation: QuickCommandConfirmationPolicy::Inherit,
            sort_order: 0,
            created_at: 1,
            updated_at: 1,
        };
        let values = BTreeMap::from([(
            "value".to_string(),
            Zeroizing::new("a b'$(touch nope)".to_string()),
        )]);
        let target = QuickCommandTargetContext {
            target_id: "local".to_string(),
            label: "Local".to_string(),
            protocol: QuickCommandTargetProtocol::Local,
            values: QuickCommandContextValues::default(),
        };

        let prepared = prepare_quick_command(&command, &[target], &values).unwrap();

        assert_eq!(
            prepared.targets[0].command.as_str(),
            "printf '%s\\n' 'a b'\"'\"'$(touch nope)'"
        );
    }

    #[test]
    fn preparation_rejects_unknown_values_and_confirms_expanded_risk() {
        let command = QuickCommand {
            id: "action".to_string(),
            name: "Action".to_string(),
            command: "{{param.action}}".to_string(),
            category: "custom".to_string(),
            description: None,
            parameters: vec![QuickCommandParameter {
                name: "action".to_string(),
                label: "Action".to_string(),
                required: true,
                ..QuickCommandParameter::default()
            }],
            availability: QuickCommandAvailability::default(),
            confirmation: QuickCommandConfirmationPolicy::Inherit,
            sort_order: 0,
            created_at: 1,
            updated_at: 1,
        };
        let target = QuickCommandTargetContext {
            target_id: "ssh".to_string(),
            label: "SSH".to_string(),
            protocol: QuickCommandTargetProtocol::Ssh,
            values: QuickCommandContextValues::default(),
        };
        let unknown_values = BTreeMap::from([
            ("action".to_string(), Zeroizing::new("uptime".to_string())),
            ("typo".to_string(), Zeroizing::new("ignored".to_string())),
        ]);

        let Err(errors) =
            prepare_quick_command(&command, std::slice::from_ref(&target), &unknown_values)
        else {
            panic!("unknown parameter values must be rejected");
        };
        assert!(matches!(
            errors.as_slice(),
            [QuickCommandTemplateError::UnknownParameter(parameter)] if parameter == "typo"
        ));

        let risky_values = BTreeMap::from([(
            "action".to_string(),
            Zeroizing::new("rm -rf /tmp/example".to_string()),
        )]);
        let prepared =
            prepare_quick_command(&command, std::slice::from_ref(&target), &risky_values).unwrap();
        assert!(prepared.confirmation_required);
        assert_eq!(prepared.targets[0].risk, Some(QuickCommandRisk::High));

        let oversized_values = BTreeMap::from([(
            "action".to_string(),
            Zeroizing::new("x".repeat(MAX_QUICK_COMMAND_ARGUMENT_VALUE_BYTES + 1)),
        )]);
        let Err(errors) =
            prepare_quick_command(&command, std::slice::from_ref(&target), &oversized_values)
        else {
            panic!("oversized parameter values must be rejected");
        };
        assert!(errors.iter().any(|error| matches!(
            error,
            QuickCommandTemplateError::ParameterValueTooLong(parameter) if parameter == "action"
        )));

        let mut amplified_command = command.clone();
        amplified_command.command = "{{param.action}}".repeat(9);
        let maximum_value = BTreeMap::from([(
            "action".to_string(),
            Zeroizing::new("x".repeat(MAX_QUICK_COMMAND_ARGUMENT_VALUE_BYTES)),
        )]);
        let Err(errors) = prepare_quick_command(&amplified_command, &[target], &maximum_value)
        else {
            panic!("expanded commands above the output limit must be rejected");
        };
        assert!(errors.iter().any(|error| matches!(
            error,
            QuickCommandTemplateError::ExpandedCommandTooLong { .. }
        )));
    }

    #[test]
    fn preparation_separates_available_and_unavailable_targets() {
        let command = QuickCommand {
            id: "ssh-only".to_string(),
            name: "SSH only".to_string(),
            command: "echo {{ctx.host}}".to_string(),
            category: "custom".to_string(),
            description: None,
            parameters: Vec::new(),
            availability: QuickCommandAvailability {
                protocols: vec![QuickCommandTargetProtocol::Ssh],
                host_patterns: Vec::new(),
            },
            confirmation: QuickCommandConfirmationPolicy::Inherit,
            sort_order: 0,
            created_at: 1,
            updated_at: 1,
        };
        let targets = [
            QuickCommandTargetContext {
                target_id: "local".to_string(),
                label: "Local".to_string(),
                protocol: QuickCommandTargetProtocol::Local,
                values: QuickCommandContextValues::default(),
            },
            QuickCommandTargetContext {
                target_id: "ssh".to_string(),
                label: "Remote".to_string(),
                protocol: QuickCommandTargetProtocol::Ssh,
                values: QuickCommandContextValues {
                    host: Some(Zeroizing::new("server.example.com".to_string())),
                    ..QuickCommandContextValues::default()
                },
            },
        ];

        let prepared = prepare_quick_command(&command, &targets, &BTreeMap::new()).unwrap();

        assert_eq!(prepared.targets.len(), 1);
        assert_eq!(
            prepared.targets[0].command.as_str(),
            "echo server.example.com"
        );
        assert_eq!(prepared.unavailable_targets, ["Local"]);
    }

    #[test]
    fn preparation_matches_the_same_target_identity_fields_it_executes() {
        let command = QuickCommand {
            id: "alice-only".to_string(),
            name: "Alice only".to_string(),
            command: "whoami".to_string(),
            category: "custom".to_string(),
            description: None,
            parameters: Vec::new(),
            availability: QuickCommandAvailability {
                protocols: vec![QuickCommandTargetProtocol::Ssh],
                host_patterns: vec!["alice@*.example.com".to_string()],
            },
            confirmation: QuickCommandConfirmationPolicy::Inherit,
            sort_order: 0,
            created_at: 1,
            updated_at: 1,
        };
        let target = QuickCommandTargetContext {
            target_id: "ssh".to_string(),
            label: "Production".to_string(),
            protocol: QuickCommandTargetProtocol::Ssh,
            values: QuickCommandContextValues {
                host: Some(Zeroizing::new("api.example.com".to_string())),
                username: Some(Zeroizing::new("alice".to_string())),
                ..QuickCommandContextValues::default()
            },
        };

        let prepared = prepare_quick_command(&command, &[target], &BTreeMap::new()).unwrap();

        assert_eq!(prepared.targets.len(), 1);
        assert!(prepared.unavailable_targets.is_empty());
    }

    #[test]
    fn noninteractive_commands_require_defaults_for_required_parameters() {
        let mut command = QuickCommand {
            id: "service".to_string(),
            name: "Service".to_string(),
            command: "systemctl status {{param.service|sh}}".to_string(),
            category: "custom".to_string(),
            description: None,
            parameters: vec![QuickCommandParameter {
                name: "service".to_string(),
                label: "Service".to_string(),
                required: true,
                ..QuickCommandParameter::default()
            }],
            availability: QuickCommandAvailability::default(),
            confirmation: QuickCommandConfirmationPolicy::Inherit,
            sort_order: 0,
            created_at: 1,
            updated_at: 1,
        };

        assert!(!quick_command_can_run_non_interactively(&command));
        command.parameters[0].default_value = Some("sshd".to_string());
        assert!(quick_command_can_run_non_interactively(&command));
    }

    #[test]
    fn runtime_substitution_detection_ignores_escaped_tokens() {
        assert!(quick_command_has_runtime_substitutions(
            "echo {{ param.name | sh }}"
        ));
        assert!(quick_command_has_runtime_substitutions("echo {{ctx.host}}"));
        assert!(!quick_command_has_runtime_substitutions(
            "echo \\{{param.name}}"
        ));
    }
}
