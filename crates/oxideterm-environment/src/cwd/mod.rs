// Copyright (C) 2026 OxideTerm contributors.
// SPDX-License-Identifier: GPL-3.0-only

mod local;
mod model;

pub(crate) use local::expand_local_home_path;
pub use local::{
    current_directory_path_is_explicit, list_local_current_directory,
    sort_current_directory_entries,
};
pub use model::{
    CurrentDirectoryEntry, CurrentDirectoryEntryKind, CurrentDirectoryKey, CurrentDirectoryScope,
    CurrentDirectorySnapshot, CurrentDirectorySource, current_directory_cd_command,
    current_directory_parent, current_directory_report_command,
    current_directory_shell_integration_command, current_directory_shell_path_argument,
};
