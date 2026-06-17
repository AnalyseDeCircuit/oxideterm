// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use crate::{
    X11AuthMaterial, X11AuthorityMatchContext, X11BinaryAuthorityEntry, X11ForwardConfig,
    X11ForwardingError, X11Result, X11SetupRequest, X11SshRequest, rewrite_setup_authentication,
    select_xauth_entry, select_xauthority_entry,
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct X11ForwardPlan {
    pub config: X11ForwardConfig,
    pub auth: X11AuthMaterial,
}

impl X11ForwardPlan {
    pub fn new(config: X11ForwardConfig, auth: X11AuthMaterial) -> Self {
        Self { config, auth }
    }

    pub fn from_xauth_entries(
        config: X11ForwardConfig,
        entries: &[crate::X11AuthEntry],
    ) -> X11Result<Self> {
        let entry = select_xauth_entry(entries, &config.local_display)
            .ok_or(X11ForwardingError::MissingAuthEntry)?;
        Ok(Self::new(
            config,
            X11AuthMaterial::mit_magic_cookie(entry.cookie.clone()),
        ))
    }

    pub fn from_binary_authority_entries(
        config: X11ForwardConfig,
        entries: &[X11BinaryAuthorityEntry],
        context: &X11AuthorityMatchContext,
    ) -> X11Result<Self> {
        let entry = select_xauthority_entry(entries, &config.local_display, context)
            .ok_or(X11ForwardingError::MissingAuthEntry)?;
        Ok(Self::new(
            config,
            X11AuthMaterial::mit_magic_cookie(entry.cookie.clone()),
        ))
    }

    pub fn remote_display_value(&self) -> String {
        self.config.remote_display_value()
    }

    pub fn ssh_request(&self) -> X11SshRequest {
        self.config.ssh_request(&self.auth)
    }

    pub fn rewrite_setup_authentication(&self, packet: &mut Vec<u8>) -> X11Result<X11SetupRequest> {
        rewrite_setup_authentication(packet, &self.auth)
    }
}

#[cfg(test)]
mod tests {
    use crate::{
        X11AuthCookie, X11AuthProtocol, X11AuthorityFamily, X11AuthorityMatchContext,
        X11BinaryAuthorityEntry, X11Display,
    };

    use super::*;

    #[test]
    fn plan_can_use_binary_xauthority_entries() {
        let display = X11Display::parse(":0").unwrap();
        let config = X11ForwardConfig::new(display);
        let entries = vec![X11BinaryAuthorityEntry {
            family: X11AuthorityFamily::Wild,
            address: Vec::new(),
            display_number: "0".to_string(),
            protocol: X11AuthProtocol::MitMagicCookie1,
            cookie: X11AuthCookie::from_hex("bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb").unwrap(),
        }];

        let plan = X11ForwardPlan::from_binary_authority_entries(
            config,
            &entries,
            &X11AuthorityMatchContext::new(),
        )
        .unwrap();

        assert_eq!(
            plan.auth.local_cookie.to_hex(),
            "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"
        );
        assert_ne!(plan.auth.fake_cookie, plan.auth.local_cookie);
    }
}
