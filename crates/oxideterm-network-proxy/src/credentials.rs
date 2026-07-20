// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use oxideterm_connections::ConnectionStore;
use zeroize::Zeroizing;

use crate::ApplicationProxyError;

pub trait ApplicationProxyCredentialProvider {
    fn application_proxy_password(
        &self,
        keychain_id: &str,
    ) -> Result<Zeroizing<String>, ApplicationProxyError>;
}

impl ApplicationProxyCredentialProvider for ConnectionStore {
    fn application_proxy_password(
        &self,
        keychain_id: &str,
    ) -> Result<Zeroizing<String>, ApplicationProxyError> {
        // Keychain implementation details and identifiers stay behind this
        // boundary; callers receive only a zeroizing runtime value.
        self.get_global_upstream_proxy_password(keychain_id)
            .map(|password| password.into_zeroizing())
            .map_err(|_| {
                ApplicationProxyError::Unavailable(
                    "the application proxy password is unavailable".to_string(),
                )
            })
    }
}
