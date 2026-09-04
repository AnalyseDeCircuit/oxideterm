// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

//! Allocation-free command head parsing shared by output-specific classifiers.

use std::str::SplitWhitespace;

const MAX_COMMAND_PREFIX_TOKENS: usize = 8;

#[derive(Clone)]
pub(crate) struct ParsedCommand<'a> {
    executable: &'a str,
    arguments: SplitWhitespace<'a>,
}

impl<'a> ParsedCommand<'a> {
    pub(crate) fn parse(command: &'a str) -> Option<Self> {
        let mut arguments = command.split_whitespace();
        let mut executable = arguments.next()?;
        // Prefix handling is bounded because command marks may contain arbitrary user input.
        for _ in 0..MAX_COMMAND_PREFIX_TOKENS {
            if matches!(executable, "sudo" | "command") || is_environment_assignment(executable) {
                executable = arguments.next()?;
            } else if executable == "env" {
                executable = arguments.find(|token| !is_environment_assignment(token))?;
            } else {
                break;
            }
        }
        let executable = executable.rsplit(['/', '\\']).next().unwrap_or(executable);
        let executable = executable.strip_suffix(".exe").unwrap_or(executable);
        Some(Self {
            executable,
            arguments,
        })
    }

    pub(crate) fn executable(&self) -> &'a str {
        self.executable
    }

    pub(crate) fn arguments(&self) -> SplitWhitespace<'a> {
        self.arguments.clone()
    }
}

fn is_environment_assignment(token: &str) -> bool {
    token
        .split_once('=')
        .is_some_and(|(name, _)| !name.is_empty() && !name.contains(['/', '\\']))
}
