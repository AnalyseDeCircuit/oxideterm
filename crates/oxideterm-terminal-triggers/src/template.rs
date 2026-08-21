// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use zeroize::Zeroizing;

use crate::{
    LocalProcessSpec, TerminalTriggerAction, TerminalTriggerError, TriggerMatched,
    compiler::MAX_ACTION_FIELD_BYTES,
};

const MAX_EXPANDED_ACTION_BYTES: usize = 64 * 1024;

/// An expanded action whose output-derived fields are zeroized on drop.
pub enum ExpandedTriggerAction {
    SendText {
        text: Zeroizing<String>,
        append_enter: bool,
    },
    RunQuickCommand {
        quick_command_id: String,
    },
    LaunchLocalProcess {
        process: ExpandedLocalProcessSpec,
    },
}

/// An expanded local process specification that preserves argument boundaries.
pub enum ExpandedLocalProcessSpec {
    DirectProgram {
        executable: Zeroizing<String>,
        arguments: Vec<Zeroizing<String>>,
        working_directory: Option<Zeroizing<String>>,
    },
    ExplicitShell {
        shell_executable: Zeroizing<String>,
        arguments: Vec<Zeroizing<String>>,
        working_directory: Option<Zeroizing<String>>,
    },
}

impl TerminalTriggerAction {
    pub fn expand(
        &self,
        matched: &TriggerMatched,
    ) -> Result<ExpandedTriggerAction, TerminalTriggerError> {
        match self {
            Self::SendText { text, append_enter } => Ok(ExpandedTriggerAction::SendText {
                text: expand_template(text, matched)?,
                append_enter: *append_enter,
            }),
            Self::RunQuickCommand { quick_command_id } => {
                Ok(ExpandedTriggerAction::RunQuickCommand {
                    quick_command_id: quick_command_id.clone(),
                })
            }
            Self::LaunchLocalProcess { process } => Ok(ExpandedTriggerAction::LaunchLocalProcess {
                process: expand_process(process, matched)?,
            }),
        }
    }
}

fn expand_process(
    process: &LocalProcessSpec,
    matched: &TriggerMatched,
) -> Result<ExpandedLocalProcessSpec, TerminalTriggerError> {
    match process {
        LocalProcessSpec::DirectProgram {
            executable,
            arguments,
            working_directory,
        } => Ok(ExpandedLocalProcessSpec::DirectProgram {
            executable: Zeroizing::new(executable.clone()),
            arguments: expand_arguments(arguments, matched)?,
            working_directory: clone_optional(working_directory.as_deref()),
        }),
        LocalProcessSpec::ExplicitShell {
            shell_executable,
            arguments,
            working_directory,
        } => Ok(ExpandedLocalProcessSpec::ExplicitShell {
            shell_executable: Zeroizing::new(shell_executable.clone()),
            arguments: expand_arguments(arguments, matched)?,
            working_directory: clone_optional(working_directory.as_deref()),
        }),
    }
}

fn expand_arguments(
    arguments: &[String],
    matched: &TriggerMatched,
) -> Result<Vec<Zeroizing<String>>, TerminalTriggerError> {
    arguments
        .iter()
        .map(|argument| expand_template(argument, matched))
        .collect()
}

fn clone_optional(value: Option<&str>) -> Option<Zeroizing<String>> {
    value.map(|value| Zeroizing::new(value.to_owned()))
}

pub fn expand_template(
    template: &str,
    matched: &TriggerMatched,
) -> Result<Zeroizing<String>, TerminalTriggerError> {
    let mut expanded = Zeroizing::new(String::with_capacity(template.len()));
    let mut cursor = 0;
    while let Some(relative_start) = template[cursor..].find("${") {
        let start = cursor + relative_start;
        expanded.push_str(&template[cursor..start]);
        let name_start = start + 2;
        let Some(relative_end) = template[name_start..].find('}') else {
            return Err(TerminalTriggerError::InvalidTemplate);
        };
        let end = name_start + relative_end;
        let name = &template[name_start..end];
        let Some(value) = matched.capture(name) else {
            return Err(TerminalTriggerError::UnknownCapture(name.to_string()));
        };
        expanded.push_str(value);
        if expanded.len() > MAX_EXPANDED_ACTION_BYTES {
            return Err(TerminalTriggerError::ExpandedActionTooLarge);
        }
        cursor = end + 1;
    }
    expanded.push_str(&template[cursor..]);
    if expanded.len() > MAX_EXPANDED_ACTION_BYTES {
        return Err(TerminalTriggerError::ExpandedActionTooLarge);
    }
    Ok(expanded)
}

pub(crate) fn template_variables(template: &str) -> Result<Vec<&str>, TerminalTriggerError> {
    if template.len() > MAX_ACTION_FIELD_BYTES {
        return Err(TerminalTriggerError::FieldTooLong {
            field: "action.template",
            limit: MAX_ACTION_FIELD_BYTES,
        });
    }
    let mut variables = Vec::new();
    let mut cursor = 0;
    while let Some(relative_start) = template[cursor..].find("${") {
        let name_start = cursor + relative_start + 2;
        let Some(relative_end) = template[name_start..].find('}') else {
            return Err(TerminalTriggerError::InvalidTemplate);
        };
        let end = name_start + relative_end;
        let name = &template[name_start..end];
        if !valid_variable_name(name) {
            return Err(TerminalTriggerError::InvalidTemplate);
        }
        variables.push(name);
        cursor = end + 1;
    }
    Ok(variables)
}

fn valid_variable_name(name: &str) -> bool {
    let mut characters = name.chars();
    let Some(first) = characters.next() else {
        return false;
    };
    (first == '_' || first.is_ascii_alphabetic())
        && characters.all(|character| character == '_' || character.is_ascii_alphanumeric())
}
