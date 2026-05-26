// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use serde::Serialize;

use crate::{
    args::WriteArgs,
    backup::{CreatedBackup, create_backup_file},
    error::{CliError, CliResult},
};

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct WriteGuardPlan {
    pub(crate) dry_run: bool,
    pub(crate) applied: bool,
    pub(crate) backup_path: Option<String>,
    pub(crate) backup_size_bytes: Option<u64>,
}

pub(crate) fn prepare_write(args: &WriteArgs, has_changes: bool) -> CliResult<WriteGuardPlan> {
    if args.dry_run || !has_changes {
        return Ok(WriteGuardPlan {
            dry_run: args.dry_run,
            applied: false,
            backup_path: None,
            backup_size_bytes: None,
        });
    }
    if !args.yes {
        return Err(CliError::new(
            "confirmation_required",
            "write command requires --yes, or use --dry-run to preview changes",
            args.json,
        ));
    }

    let backup = if should_backup(args) {
        Some(create_backup_file(None, args.json)?)
    } else {
        None
    };
    Ok(write_plan_from_backup(args, backup))
}

fn should_backup(args: &WriteArgs) -> bool {
    // Backups are on by default for invasive commands; the explicit flag is accepted for scripts.
    !args.no_backup || args.backup_before_write
}

fn write_plan_from_backup(args: &WriteArgs, backup: Option<CreatedBackup>) -> WriteGuardPlan {
    WriteGuardPlan {
        dry_run: args.dry_run,
        applied: false,
        backup_path: backup.as_ref().map(|backup| backup.path.clone()),
        backup_size_bytes: backup.map(|backup| backup.size_bytes),
    }
}

pub(crate) fn mark_applied(plan: &mut WriteGuardPlan) {
    plan.applied = true;
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write_args(dry_run: bool, yes: bool, no_backup: bool) -> WriteArgs {
        WriteArgs {
            dry_run,
            yes,
            no_backup,
            backup_before_write: false,
            json: true,
        }
    }

    #[test]
    fn dry_run_does_not_require_confirmation() {
        let plan = prepare_write(&write_args(true, false, false), true).unwrap();

        assert!(plan.dry_run);
        assert!(!plan.applied);
        assert!(plan.backup_path.is_none());
    }

    #[test]
    fn real_write_requires_confirmation() {
        let error = prepare_write(&write_args(false, false, false), true).unwrap_err();

        assert_eq!(error.code, "confirmation_required");
    }
}
