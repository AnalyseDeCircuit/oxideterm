// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use clap::CommandFactory;
use clap_complete::{Shell, generate};

use crate::args::{Cli, CompletionArgs, CompletionShell};

pub(crate) fn run(args: CompletionArgs) {
    let mut command = Cli::command();
    let binary_name = command.get_name().to_string();
    let shell: Shell = args.shell.into();
    // Completion scripts are generated to stdout so shell installers and CI
    // jobs can redirect them without touching OxideTerm state.
    generate(shell, &mut command, binary_name, &mut std::io::stdout());
}

impl From<CompletionShell> for Shell {
    fn from(shell: CompletionShell) -> Self {
        match shell {
            CompletionShell::Bash => Self::Bash,
            CompletionShell::Elvish => Self::Elvish,
            CompletionShell::Fish => Self::Fish,
            CompletionShell::PowerShell => Self::PowerShell,
            CompletionShell::Zsh => Self::Zsh,
        }
    }
}
