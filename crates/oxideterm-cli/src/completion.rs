// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use std::{fs, path::PathBuf};

use clap::CommandFactory;
use clap_complete::{Shell, generate};

use crate::{
    args::{Cli, CompletionAction, CompletionArgs, CompletionShell},
    error::{CliError, CliResult},
    output,
};

pub(crate) fn run(args: CompletionArgs) -> CliResult<()> {
    match args.action {
        Some(CompletionAction::Generate(args)) => generate_completion(args.shell),
        Some(CompletionAction::Path(args)) => {
            output::write_text(completion_path(args.shell)?.display().to_string());
            Ok(())
        }
        Some(CompletionAction::Install(args)) => install_completion(args.shell, args.force),
        None => {
            let shell = args.shell.ok_or_else(|| {
                CliError::new(
                    "completion_shell_missing",
                    "completion requires a shell or a subcommand",
                    false,
                )
            })?;
            generate_completion(shell)
        }
    }
}

fn generate_completion(shell: CompletionShell) -> CliResult<()> {
    let mut command = Cli::command();
    let binary_name = command.get_name().to_string();
    let shell: Shell = shell.into();
    // Completion scripts are generated to stdout so shell installers and CI
    // jobs can redirect them without touching OxideTerm state.
    generate(shell, &mut command, binary_name, &mut std::io::stdout());
    Ok(())
}

fn install_completion(shell: CompletionShell, force: bool) -> CliResult<()> {
    let path = completion_path(shell)?;
    if path.exists() && !force {
        return Err(CliError::new(
            "completion_install_exists",
            format!(
                "completion file {} already exists; pass --force to overwrite it",
                path.display()
            ),
            false,
        ));
    }
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| {
            CliError::new(
                "completion_install_failed",
                format!(
                    "failed to create completion directory {}: {error}",
                    parent.display()
                ),
                false,
            )
        })?;
    }

    let mut command = Cli::command();
    let mut bytes = Vec::new();
    let generator: Shell = shell.into();
    generate(generator, &mut command, "oxideterm", &mut bytes);
    fs::write(&path, bytes).map_err(|error| {
        CliError::new(
            "completion_install_failed",
            format!(
                "failed to write completion file {}: {error}",
                path.display()
            ),
            false,
        )
    })?;
    output::write_text(format!("installed: {}", path.display()));
    Ok(())
}

fn completion_path(shell: CompletionShell) -> CliResult<PathBuf> {
    let home = home_dir().ok_or_else(|| {
        CliError::new(
            "home_dir_not_found",
            "could not resolve home directory for completion path",
            false,
        )
    })?;
    Ok(match shell {
        CompletionShell::Bash => home.join(".local/share/bash-completion/completions/oxideterm"),
        CompletionShell::Elvish => home.join(".elvish/lib/oxideterm-completions.elv"),
        CompletionShell::Fish => home.join(".config/fish/completions/oxideterm.fish"),
        CompletionShell::PowerShell => home.join("Documents/PowerShell/Completions/oxideterm.ps1"),
        CompletionShell::Zsh => home.join(".zfunc/_oxideterm"),
    })
}

fn home_dir() -> Option<PathBuf> {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .filter(|path| !path.as_os_str().is_empty())
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn completion_paths_use_shell_specific_file_names() {
        let zsh = completion_path(CompletionShell::Zsh).unwrap();
        assert!(path_ends_with(&zsh, ".zfunc/_oxideterm"));

        let fish = completion_path(CompletionShell::Fish).unwrap();
        assert!(path_ends_with(
            &fish,
            ".config/fish/completions/oxideterm.fish"
        ));
    }

    fn path_ends_with(path: &Path, suffix: &str) -> bool {
        path.display().to_string().ends_with(suffix)
    }
}
