// Copyright (C) 2026 OxideTerm contributors.
// SPDX-License-Identifier: GPL-3.0-only

mod model;
mod parse;
mod probe;
mod store;

pub use model::{
    ProjectFacet, ProjectFacetKind, ProjectManifestEntry, ProjectProbeError, ProjectProbeKey,
    ProjectProbeOutcome, ProjectProbeScope, ProjectSnapshot, ProjectTask, ProjectTaskGroup,
};
pub use parse::{interpret_project_manifest_entries, project_manifest_file_names};
pub use probe::{
    PROJECT_PROBE_MAX_ANCESTORS, PROJECT_PROBE_MAX_FILE_BYTES, PROJECT_SHELL_PROBE_SENTINEL,
    parse_remote_shell_project_probe_output, probe_local_project,
    remote_project_cwd_source_is_trusted, remote_shell_project_probe_command,
};
pub use store::{ProjectProbeEntry, ProjectProbeState, ProjectStatusStore};
