// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use libgssapi::{
    context::{ClientCtx, CtxFlags, SecurityContext},
    error::{Error as GssError, MajorFlags},
    name::Name,
    oid::{GSS_MECH_KRB5, GSS_NT_HOSTBASED_SERVICE},
    util::Buf,
};
use russh::GssapiStep;
use thiserror::Error;
use zeroize::{Zeroize, Zeroizing};

pub(super) struct PlatformContext {
    inner: ClientCtx,
}

impl std::fmt::Debug for PlatformContext {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("PlatformContext([redacted Kerberos context])")
    }
}

#[derive(Debug, Error)]
pub(crate) enum PlatformError {
    #[error("no Kerberos credentials are available")]
    NoCredentials,
    #[error("the Kerberos credentials have expired")]
    CredentialsExpired,
    #[error("the Kerberos service is unavailable")]
    ServiceUnavailable,
    #[error("the Kerberos server identity was rejected")]
    ServerIdentityRejected,
    #[error("the Kerberos context does not provide integrity protection")]
    IntegrityUnavailable,
    #[error("the Kerberos server did not accept credential delegation")]
    DelegationUnavailable,
    #[error("the Kerberos mechanism returned no continuation token")]
    MissingContinuationToken,
    #[error("the platform GSSAPI operation failed")]
    Other,
}

impl From<GssError> for PlatformError {
    fn from(error: GssError) -> Self {
        if error.major.contains(MajorFlags::GSS_S_NO_CRED) {
            Self::NoCredentials
        } else if error
            .major
            .intersects(MajorFlags::GSS_S_CREDENTIALS_EXPIRED | MajorFlags::GSS_S_CONTEXT_EXPIRED)
        {
            Self::CredentialsExpired
        } else if error
            .major
            .intersects(MajorFlags::GSS_S_BAD_NAME | MajorFlags::GSS_S_BAD_NAMETYPE)
        {
            Self::ServerIdentityRejected
        } else if error.major.contains(MajorFlags::GSS_S_UNAVAILABLE) {
            Self::ServiceUnavailable
        } else {
            Self::Other
        }
    }
}

fn copy_and_wipe(mut buffer: Buf) -> Zeroizing<Vec<u8>> {
    let copy = Zeroizing::new(buffer.to_vec());
    buffer.zeroize();
    copy
}

fn new_context(
    server_identity: &str,
    delegate_credentials: bool,
) -> Result<PlatformContext, PlatformError> {
    let target = if server_identity.starts_with("host@") {
        server_identity.to_string()
    } else {
        format!("host@{server_identity}")
    };
    let target = Name::new(target.as_bytes(), Some(GSS_NT_HOSTBASED_SERVICE))?;
    let mut flags = CtxFlags::GSS_C_INTEG_FLAG;
    if delegate_credentials {
        flags.insert(CtxFlags::GSS_C_DELEG_FLAG);
    }
    Ok(PlatformContext {
        inner: ClientCtx::new(None, target, flags, Some(GSS_MECH_KRB5)),
    })
}

pub(super) fn advance(
    context: Option<PlatformContext>,
    server_identity: &str,
    delegate_credentials: bool,
    input_token: Option<Zeroizing<Vec<u8>>>,
    mic_data: Zeroizing<Vec<u8>>,
) -> Result<(PlatformContext, GssapiStep), PlatformError> {
    let mut context = match context {
        Some(context) => context,
        None => new_context(server_identity, delegate_credentials)?,
    };
    let output_token = context
        .inner
        .step(input_token.as_deref().map(Vec::as_slice), None)?;

    if !context.inner.is_complete() {
        let token = output_token
            .filter(|token| !token.is_empty())
            .map(copy_and_wipe)
            .ok_or(PlatformError::MissingContinuationToken)?;
        return Ok((context, GssapiStep::Continue { token }));
    }

    if !context.inner.flags()?.contains(CtxFlags::GSS_C_INTEG_FLAG) {
        return Err(PlatformError::IntegrityUnavailable);
    }
    if delegate_credentials && !context.inner.flags()?.contains(CtxFlags::GSS_C_DELEG_FLAG) {
        return Err(PlatformError::DelegationUnavailable);
    }
    let token = output_token
        .filter(|token| !token.is_empty())
        .map(copy_and_wipe);
    let mic = copy_and_wipe(context.inner.get_mic(&mic_data)?);
    Ok((
        context,
        GssapiStep::Complete {
            token,
            mic: Some(mic),
        },
    ))
}
