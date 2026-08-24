// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use std::collections::{BTreeMap, HashMap};
use zeroize::Zeroizing;

use crate::{
    QuickCommand, QuickCommandConfirmationPolicy, QuickCommandParameter, QuickCommandParameterKind,
    QuickCommandRisk, QuickCommandTargetProtocol, classify_command_risk,
    quick_command_available_for_target,
};

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
    UnknownParameter(String),
    MissingParameter(String),
    InvalidChoice { parameter: String },
    MissingContext { target: String, field: String },
}

impl std::fmt::Display for QuickCommandTemplateError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnterminatedToken => formatter.write_str("unterminated template token"),
            Self::UnknownToken(_) => formatter.write_str("unknown template token"),
            Self::UnknownParameter(parameter) => {
                write!(formatter, "unknown parameter {parameter}")
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
        if !quick_command_available_for_target(
            command,
            target.protocol,
            target.values.host.as_deref().map(String::as_str),
        ) {
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

fn validate_parameter_values(
    parameters: &HashMap<&str, &QuickCommandParameter>,
    parameter_values: &BTreeMap<String, Zeroizing<String>>,
    errors: &mut Vec<QuickCommandTemplateError>,
) {
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
            resolved.push_str("{{");
            cursor += 3;
            continue;
        }
        let Some(token_offset) = remainder.find("{{") else {
            resolved.push_str(remainder);
            break;
        };
        resolved.push_str(&remainder[..token_offset]);
        cursor += token_offset + 2;
        let Some(token_end) = template[cursor..].find("}}") else {
            errors.push(QuickCommandTemplateError::UnterminatedToken);
            break;
        };
        let token = template[cursor..cursor + token_end].trim();
        cursor += token_end + 2;
        match resolve_token(token, parameters, parameter_values, target) {
            Ok(value) => resolved.push_str(&value),
            Err(error) => errors.push(error),
        }
    }
    if errors.is_empty() {
        Ok(resolved)
    } else {
        Err(errors)
    }
}

fn resolve_token(
    token: &str,
    parameters: &HashMap<&str, &QuickCommandParameter>,
    parameter_values: &BTreeMap<String, Zeroizing<String>>,
    target: &QuickCommandTargetContext,
) -> Result<Zeroizing<String>, QuickCommandTemplateError> {
    if let Some(name) = token.strip_prefix("param.") {
        let Some(parameter) = parameters.get(name) else {
            return Err(QuickCommandTemplateError::UnknownParameter(
                name.to_string(),
            ));
        };
        return Ok(parameter_values
            .get(name)
            .filter(|value| !value.is_empty())
            .cloned()
            .or_else(|| parameter.default_value.clone().map(Zeroizing::new))
            .unwrap_or_else(|| Zeroizing::new(String::new())));
    }
    if let Some(field) = token.strip_prefix("ctx.") {
        return context_value(&target.values, field).ok_or_else(|| {
            QuickCommandTemplateError::MissingContext {
                target: target.label.clone(),
                field: field.to_string(),
            }
        });
    }
    Err(QuickCommandTemplateError::UnknownToken(token.to_string()))
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
}
