// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use super::*;

#[test]
fn import_defaults_to_dry_run_without_yes() {
    let write = effective_import_write(WriteArgs {
        dry_run: false,
        yes: false,
        no_backup: false,
        backup_before_write: false,
        json: true,
        format: None,
    });

    assert!(write.dry_run);
}
