// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use std::path::{Path, PathBuf};

use crate::{
    X11AuthorityEnvironment, X11AuthorityFile, X11AuthorityMatchContext, X11ForwardConfig,
    X11ForwardPlan, X11ForwardingError, X11Result, parse_xauthority_file,
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct X11LocalAuthorityResolver {
    pub env: X11AuthorityEnvironment,
    pub context: X11AuthorityMatchContext,
}

impl X11LocalAuthorityResolver {
    pub fn from_process_env() -> Self {
        Self {
            env: X11AuthorityEnvironment::from_process_env(),
            context: X11AuthorityMatchContext::new(),
        }
    }

    pub fn new(env: X11AuthorityEnvironment, context: X11AuthorityMatchContext) -> Self {
        Self { env, context }
    }

    pub fn authority_path(&self) -> X11Result<PathBuf> {
        match &self.env.authority_file {
            X11AuthorityFile::Path(path) => Ok(expand_tilde(path)),
            X11AuthorityFile::Default => default_xauthority_path()
                .ok_or_else(|| X11ForwardingError::AuthorityFileUnavailable("default".to_string())),
        }
    }

    pub fn resolve_from_file(&self, config: X11ForwardConfig) -> X11Result<X11ForwardPlan> {
        self.resolve_from_file_at(config, self.authority_path()?)
    }

    pub fn resolve_from_file_at(
        &self,
        config: X11ForwardConfig,
        path: impl AsRef<Path>,
    ) -> X11Result<X11ForwardPlan> {
        let bytes = std::fs::read(path.as_ref())
            .map_err(|error| X11ForwardingError::AuthorityFileUnavailable(error.to_string()))?;
        let entries = parse_xauthority_file(&bytes)?;
        X11ForwardPlan::from_binary_authority_entries(config, &entries, &self.context)
    }
}

fn default_xauthority_path() -> Option<PathBuf> {
    if let Some(path) = std::env::var_os("XAUTHORITY").filter(|value| !value.is_empty()) {
        return Some(PathBuf::from(path));
    }
    std::env::home_dir().map(|home| home.join(".Xauthority"))
}

fn expand_tilde(value: &str) -> PathBuf {
    if value == "~" {
        return std::env::home_dir().unwrap_or_else(|| PathBuf::from(value));
    }
    if let Some(rest) = value.strip_prefix("~/")
        && let Some(home) = std::env::home_dir()
    {
        return home.join(rest);
    }
    PathBuf::from(value)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn explicit_authority_path_is_expanded_without_reading() {
        let resolver = X11LocalAuthorityResolver::new(
            X11AuthorityEnvironment::from_values(
                Some(":0".to_string()),
                Some("/tmp/example.Xauthority".to_string()),
            ),
            X11AuthorityMatchContext::new(),
        );

        assert_eq!(
            resolver.authority_path().unwrap(),
            PathBuf::from("/tmp/example.Xauthority")
        );
    }
}
