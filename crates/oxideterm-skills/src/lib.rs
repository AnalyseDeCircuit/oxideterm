// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

//! Agent Skills discovery and bounded content loading.

mod discovery;
mod model;
mod parser;
mod registry;

pub use discovery::SkillDiscoveryOptions;
pub use model::{
    SkillCatalogEntry, SkillDiagnostic, SkillDiagnosticKind, SkillOrigin, SkillRecord, SkillScope,
};
pub use registry::{SkillRegistry, SkillRegistryError};

#[cfg(test)]
mod tests;
