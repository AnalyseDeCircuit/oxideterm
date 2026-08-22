// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use std::{ffi::c_void, ptr};

use russh::GssapiStep;
use thiserror::Error;
use windows::{
    Win32::{
        Foundation::{
            SEC_E_CONTEXT_EXPIRED, SEC_E_NO_AUTHENTICATING_AUTHORITY, SEC_E_NO_CREDENTIALS,
            SEC_E_OK, SEC_E_TARGET_UNKNOWN, SEC_E_WRONG_PRINCIPAL, SEC_I_COMPLETE_AND_CONTINUE,
            SEC_I_COMPLETE_NEEDED, SEC_I_CONTINUE_NEEDED,
        },
        Security::{
            Authentication::Identity::{
                AcquireCredentialsHandleW, CompleteAuthToken, DeleteSecurityContext,
                FreeContextBuffer, FreeCredentialsHandle, ISC_REQ_ALLOCATE_MEMORY,
                ISC_REQ_DELEGATE, ISC_REQ_INTEGRITY, ISC_REQ_MUTUAL_AUTH, ISC_RET_DELEGATE,
                ISC_RET_INTEGRITY, ISC_RET_MUTUAL_AUTH, InitializeSecurityContextW,
                MICROSOFT_KERBEROS_NAME_W, MakeSignature, QueryContextAttributesW, SECBUFFER_DATA,
                SECBUFFER_TOKEN, SECBUFFER_VERSION, SECPKG_ATTR_SIZES, SECPKG_CRED_OUTBOUND,
                SECURITY_NATIVE_DREP, SecBuffer, SecBufferDesc, SecPkgContext_Sizes,
            },
            Credentials::SecHandle,
        },
    },
    core::{HRESULT, PCWSTR},
};
use zeroize::{Zeroize, Zeroizing};

pub(super) struct PlatformContext {
    // The context must be dropped before the credential handle it depends on.
    context: ContextHandle,
    credentials: CredentialHandle,
    complete: bool,
}

impl std::fmt::Debug for PlatformContext {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("PlatformContext([redacted Kerberos context])")
    }
}

struct CredentialHandle {
    handle: SecHandle,
    valid: bool,
}

impl CredentialHandle {
    fn acquire() -> Result<Self, PlatformError> {
        let mut credentials = Self {
            handle: SecHandle::default(),
            valid: false,
        };
        // Null principal and auth data select the current Windows logon credentials.
        unsafe {
            AcquireCredentialsHandleW(
                PCWSTR::null(),
                MICROSOFT_KERBEROS_NAME_W,
                SECPKG_CRED_OUTBOUND,
                None,
                None,
                None,
                None,
                &mut credentials.handle,
                None,
            )
        }
        .map_err(|error| PlatformError::from_status(error.code()))?;
        credentials.valid = true;
        Ok(credentials)
    }
}

pub(super) fn credentials_available() -> bool {
    CredentialHandle::acquire().is_ok()
}

impl Drop for CredentialHandle {
    fn drop(&mut self) {
        if self.valid {
            let _ = unsafe { FreeCredentialsHandle(&self.handle) };
            self.handle = SecHandle::default();
            self.valid = false;
        }
    }
}

struct ContextHandle {
    handle: SecHandle,
    valid: bool,
}

impl Drop for ContextHandle {
    fn drop(&mut self) {
        if self.valid {
            let _ = unsafe { DeleteSecurityContext(&self.handle) };
            self.handle = SecHandle::default();
            self.valid = false;
        }
    }
}

struct AllocatedSspiToken {
    pointer: *mut c_void,
    length: usize,
}

impl AllocatedSspiToken {
    fn from_buffer(buffer: &mut SecBuffer) -> Self {
        let token = Self {
            pointer: buffer.pvBuffer,
            length: buffer.cbBuffer as usize,
        };
        buffer.pvBuffer = ptr::null_mut();
        buffer.cbBuffer = 0;
        token
    }

    fn copy(&self) -> Zeroizing<Vec<u8>> {
        if self.pointer.is_null() || self.length == 0 {
            return Zeroizing::new(Vec::new());
        }
        Zeroizing::new(unsafe {
            std::slice::from_raw_parts(self.pointer.cast::<u8>(), self.length).to_vec()
        })
    }
}

impl Drop for AllocatedSspiToken {
    fn drop(&mut self) {
        if self.pointer.is_null() {
            return;
        }
        if self.length > 0 {
            unsafe {
                std::slice::from_raw_parts_mut(self.pointer.cast::<u8>(), self.length).zeroize();
            }
        }
        let _ = unsafe { FreeContextBuffer(self.pointer) };
        self.pointer = ptr::null_mut();
        self.length = 0;
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
    #[error("the Kerberos token is too large")]
    TokenTooLarge,
    #[error("the platform GSSAPI operation failed")]
    Other,
}

impl PlatformError {
    fn from_status(status: HRESULT) -> Self {
        if status == SEC_E_NO_CREDENTIALS {
            Self::NoCredentials
        } else if status == SEC_E_CONTEXT_EXPIRED {
            Self::CredentialsExpired
        } else if status == SEC_E_NO_AUTHENTICATING_AUTHORITY {
            Self::ServiceUnavailable
        } else if status == SEC_E_TARGET_UNKNOWN || status == SEC_E_WRONG_PRINCIPAL {
            Self::ServerIdentityRejected
        } else {
            Self::Other
        }
    }

    pub(super) fn allows_authentication_fallback(&self) -> bool {
        matches!(
            self,
            Self::NoCredentials | Self::CredentialsExpired | Self::ServiceUnavailable
        )
    }
}

fn create_context() -> Result<PlatformContext, PlatformError> {
    Ok(PlatformContext {
        context: ContextHandle {
            handle: SecHandle::default(),
            valid: false,
        },
        credentials: CredentialHandle::acquire()?,
        complete: false,
    })
}

fn signature(
    context: &ContextHandle,
    mic_data: &Zeroizing<Vec<u8>>,
) -> Result<Zeroizing<Vec<u8>>, PlatformError> {
    let mut sizes = SecPkgContext_Sizes::default();
    unsafe {
        QueryContextAttributesW(
            &context.handle,
            SECPKG_ATTR_SIZES,
            (&mut sizes as *mut SecPkgContext_Sizes).cast(),
        )
    }
    .map_err(|error| PlatformError::from_status(error.code()))?;

    let mut mic = Zeroizing::new(vec![0; sizes.cbMaxSignature as usize]);
    let mut buffers = [
        SecBuffer {
            cbBuffer: sizes.cbMaxSignature,
            BufferType: SECBUFFER_TOKEN,
            pvBuffer: mic.as_mut_ptr().cast(),
        },
        SecBuffer {
            cbBuffer: u32::try_from(mic_data.len()).map_err(|_| PlatformError::TokenTooLarge)?,
            BufferType: SECBUFFER_DATA,
            pvBuffer: mic_data.as_ptr().cast_mut().cast(),
        },
    ];
    let message = SecBufferDesc {
        ulVersion: SECBUFFER_VERSION,
        cBuffers: buffers.len() as u32,
        pBuffers: buffers.as_mut_ptr(),
    };
    unsafe { MakeSignature(&context.handle, 0, &message, 0) }
        .map_err(|error| PlatformError::from_status(error.code()))?;
    let actual_length = buffers[0].cbBuffer as usize;
    if actual_length > mic.len() {
        return Err(PlatformError::Other);
    }
    mic.truncate(actual_length);
    Ok(mic)
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
        None => create_context()?,
    };
    if context.complete {
        return Err(PlatformError::Other);
    }

    let target_name = if server_identity.starts_with("host/") {
        server_identity.to_string()
    } else if let Some(server) = server_identity.strip_prefix("host@") {
        format!("host/{server}")
    } else {
        format!("host/{server_identity}")
    };
    let mut target = target_name.encode_utf16().collect::<Vec<_>>();
    target.push(0);

    let mut request_flags = ISC_REQ_ALLOCATE_MEMORY | ISC_REQ_INTEGRITY;
    if delegate_credentials {
        // Windows Kerberos requires mutual authentication for delegation.
        request_flags |= ISC_REQ_DELEGATE | ISC_REQ_MUTUAL_AUTH;
    }

    let mut input_buffer = input_token
        .as_ref()
        .map(|token| {
            Ok(SecBuffer {
                cbBuffer: u32::try_from(token.len()).map_err(|_| PlatformError::TokenTooLarge)?,
                BufferType: SECBUFFER_TOKEN,
                pvBuffer: token.as_ptr().cast_mut().cast(),
            })
        })
        .transpose()?;
    let input_desc = input_buffer.as_mut().map(|buffer| SecBufferDesc {
        ulVersion: SECBUFFER_VERSION,
        cBuffers: 1,
        pBuffers: buffer,
    });
    let mut output_buffer = SecBuffer {
        cbBuffer: 0,
        BufferType: SECBUFFER_TOKEN,
        pvBuffer: ptr::null_mut(),
    };
    let mut output_desc = SecBufferDesc {
        ulVersion: SECBUFFER_VERSION,
        cBuffers: 1,
        pBuffers: &mut output_buffer,
    };
    let mut returned_attributes = 0u32;
    let context_pointer = ptr::addr_of_mut!(context.context.handle);
    let status = unsafe {
        InitializeSecurityContextW(
            Some(&context.credentials.handle),
            context
                .context
                .valid
                .then_some(context_pointer.cast_const()),
            Some(target.as_ptr()),
            request_flags,
            0,
            SECURITY_NATIVE_DREP,
            input_desc.as_ref().map(|descriptor| descriptor as *const _),
            0,
            Some(context_pointer),
            Some(&mut output_desc),
            &mut returned_attributes,
            None,
        )
    };

    let recognized = status == SEC_E_OK
        || status == SEC_I_CONTINUE_NEEDED
        || status == SEC_I_COMPLETE_NEEDED
        || status == SEC_I_COMPLETE_AND_CONTINUE;
    if recognized {
        context.context.valid = true;
    }
    if status == SEC_I_COMPLETE_NEEDED || status == SEC_I_COMPLETE_AND_CONTINUE {
        unsafe { CompleteAuthToken(&context.context.handle, &output_desc) }
            .map_err(|error| PlatformError::from_status(error.code()))?;
    }
    let allocated_token = AllocatedSspiToken::from_buffer(&mut output_buffer);
    if !recognized {
        return Err(PlatformError::from_status(status));
    }
    let output_token = allocated_token.copy();

    if status == SEC_I_CONTINUE_NEEDED || status == SEC_I_COMPLETE_AND_CONTINUE {
        if output_token.is_empty() {
            return Err(PlatformError::MissingContinuationToken);
        }
        return Ok((
            context,
            GssapiStep::Continue {
                token: output_token,
            },
        ));
    }

    if returned_attributes & ISC_RET_INTEGRITY == 0 {
        return Err(PlatformError::IntegrityUnavailable);
    }
    if delegate_credentials
        && (returned_attributes & ISC_RET_DELEGATE == 0
            || returned_attributes & ISC_RET_MUTUAL_AUTH == 0)
    {
        return Err(PlatformError::DelegationUnavailable);
    }
    context.complete = true;
    let mic = signature(&context.context, &mic_data)?;
    Ok((
        context,
        GssapiStep::Complete {
            token: (!output_token.is_empty()).then_some(output_token),
            mic: Some(mic),
        },
    ))
}
