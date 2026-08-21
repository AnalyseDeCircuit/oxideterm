// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use std::fmt;

use russh::{GssapiAuthenticator, GssapiError, GssapiStep, SendError};
use thiserror::Error;
use zeroize::Zeroizing;

#[cfg(any(target_os = "linux", target_os = "macos"))]
mod unix;
#[cfg(windows)]
mod windows;

#[cfg(any(target_os = "linux", target_os = "macos"))]
use unix as platform;
#[cfg(windows)]
use windows as platform;

/// RFC 4462 transports the complete DER encoding, including the OID tag and length.
pub(crate) const KERBEROS_V5_OID_DER: &[u8] = b"\x06\x09\x2a\x86\x48\x86\xf7\x12\x01\x02\x02";

pub(crate) struct KerberosAuthenticator {
    server_identity: String,
    delegate_credentials: bool,
    context: Option<platform::PlatformContext>,
}

impl KerberosAuthenticator {
    pub(crate) fn new(
        host: &str,
        server_identity: Option<&str>,
        delegate_credentials: bool,
    ) -> Result<Self, KerberosAuthError> {
        let server_identity = server_identity.unwrap_or(host).trim();
        if server_identity.is_empty() || server_identity.contains('\0') {
            return Err(KerberosAuthError::InvalidServerIdentity);
        }
        Ok(Self {
            server_identity: server_identity.to_string(),
            delegate_credentials,
            context: None,
        })
    }

    pub(crate) fn mechanism_oids() -> Vec<Vec<u8>> {
        vec![KERBEROS_V5_OID_DER.to_vec()]
    }
}

impl fmt::Debug for KerberosAuthenticator {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("KerberosAuthenticator")
            .field("server_identity_configured", &true)
            .field("delegate_credentials", &self.delegate_credentials)
            .field("context_started", &self.context.is_some())
            .finish()
    }
}

#[derive(Debug, Error)]
pub(crate) enum KerberosAuthError {
    #[error("SSH authentication channel closed")]
    Send(#[from] SendError),
    #[error("the SSH server selected an unexpected GSSAPI mechanism")]
    UnexpectedMechanism,
    #[error("the Kerberos server identity is invalid")]
    InvalidServerIdentity,
    #[error("the platform Kerberos operation could not be scheduled")]
    WorkerUnavailable,
    #[error(transparent)]
    Platform(#[from] platform::PlatformError),
}

impl GssapiAuthenticator for KerberosAuthenticator {
    type Error = KerberosAuthError;

    async fn gssapi_step(
        &mut self,
        selected_mechanism: Option<Vec<u8>>,
        input_token: Option<Zeroizing<Vec<u8>>>,
        mic_data: Zeroizing<Vec<u8>>,
    ) -> Result<GssapiStep, Self::Error> {
        if let Some(selected_mechanism) = selected_mechanism
            && selected_mechanism.as_slice() != KERBEROS_V5_OID_DER
        {
            return Err(KerberosAuthError::UnexpectedMechanism);
        }

        let context = self.context.take();
        let server_identity = self.server_identity.clone();
        let delegate_credentials = self.delegate_credentials;
        let (context, step) = tokio::task::spawn_blocking(move || {
            platform::advance(
                context,
                &server_identity,
                delegate_credentials,
                input_token,
                mic_data,
            )
        })
        .await
        .map_err(|_| KerberosAuthError::WorkerUnavailable)??;
        self.context = Some(context);
        Ok(step)
    }

    async fn gssapi_error(&mut self, _error: GssapiError) {
        // Server-provided status text and error tokens are untrusted and may
        // contain identity data, so the integration deliberately does not log them.
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn debug_output_does_not_include_server_identity() {
        let identity = "sensitive.internal.example";
        let authenticator = KerberosAuthenticator::new("host", Some(identity), false).unwrap();

        assert!(!format!("{authenticator:?}").contains(identity));
    }
}
