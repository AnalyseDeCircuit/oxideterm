// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use clap::{Args, Subcommand};

#[derive(Debug, Args)]
#[command(long_about = "Inspect and manage OxideTerm's portable runtime keystore.")]
#[command(
    after_help = "Examples:\n  oxideterm portable status --json\n  printf '%s' \"$PORTABLE_PASSWORD\" | oxideterm portable setup --password-stdin\n  oxideterm portable unlock --password-env OXIDETERM_PORTABLE_PASSWORD\n  oxideterm portable change-password --current-password-env OLD --new-password-env NEW"
)]
pub struct PortableCommand {
    #[command(subcommand)]
    pub action: PortableAction,
}

#[derive(Debug, Subcommand)]
pub enum PortableAction {
    #[command(about = "Print portable runtime status")]
    Status(PortableStatusArgs),
    #[command(about = "Create the portable keystore")]
    Setup(PortablePasswordArgs),
    #[command(about = "Unlock the portable keystore for this process")]
    Unlock(PortablePasswordArgs),
    #[command(about = "Change the portable keystore password")]
    ChangePassword(PortableChangePasswordArgs),
    #[command(about = "Delete the portable keystore")]
    Reset(PortableResetArgs),
}

#[derive(Debug, Args)]
pub struct PortableStatusArgs {
    #[arg(long, help = "Print machine-readable JSON output")]
    pub json: bool,
}

#[derive(Debug, Args)]
pub struct PortablePasswordArgs {
    #[arg(
        long = "password-stdin",
        conflicts_with = "password_env",
        help = "Read the password from stdin"
    )]
    pub password_stdin: bool,
    #[arg(
        long = "password-env",
        value_name = "VAR",
        conflicts_with = "password_stdin",
        help = "Read the password from environment variable VAR"
    )]
    pub password_env: Option<String>,
    #[arg(long, help = "Print machine-readable JSON output")]
    pub json: bool,
}

#[derive(Debug, Args)]
pub struct PortableChangePasswordArgs {
    #[arg(
        long = "password-stdin",
        help = "Read current password from the first stdin line and new password from the second"
    )]
    pub password_stdin: bool,
    #[arg(long = "current-password-env", value_name = "VAR")]
    pub current_password_env: Option<String>,
    #[arg(long = "new-password-env", value_name = "VAR")]
    pub new_password_env: Option<String>,
    #[arg(long, help = "Print machine-readable JSON output")]
    pub json: bool,
}

#[derive(Debug, Args)]
pub struct PortableResetArgs {
    #[arg(long, help = "Actually delete keystore.vault")]
    pub yes: bool,
    #[arg(long, help = "Print machine-readable JSON output")]
    pub json: bool,
}
