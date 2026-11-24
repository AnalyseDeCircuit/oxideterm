// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

//! Agent Skills discovery and bounded content loading.

#[cfg(not(target_family = "wasm"))]
mod discovery;
mod model;
mod parser;
#[cfg(not(target_family = "wasm"))]
mod registry;

#[cfg(not(target_family = "wasm"))]
pub use discovery::SkillDiscoveryOptions;
pub use model::{
    SkillCatalogEntry, SkillDiagnostic, SkillDiagnosticKind, SkillOrigin, SkillRecord,
    SkillRegistryError, SkillScope,
};
pub use parser::{ParsedSkill, parse_skill};
#[cfg(not(target_family = "wasm"))]
pub use registry::SkillRegistry;

#[cfg(all(test, not(target_family = "wasm")))]
mod tests;
