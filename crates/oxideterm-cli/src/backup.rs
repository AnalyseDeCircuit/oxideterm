// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

mod create;
mod document;
mod list;
mod restore;
mod verify;

use crate::{
    args::{BackupAction, BackupCommand},
    error::CliResult,
};

pub(crate) use create::{CreatedBackup, create_backup_file};

pub fn run(command: BackupCommand) -> CliResult<()> {
    match command.action {
        BackupAction::Preview(args) => create::preview(args),
        BackupAction::Create(args) => create::create(args),
        BackupAction::List(args) => list::list(args),
        BackupAction::Inspect(args) => list::inspect(args),
        BackupAction::Verify(args) => verify::verify(args),
        BackupAction::Restore(args) => restore::restore(args).map(|_| ()),
    }
}
