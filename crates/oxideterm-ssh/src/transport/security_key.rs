// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

struct SecurityKeySigner {
    key: Arc<PrivateKey>,
    provider: Arc<oxideterm_security_key::SecurityKeyProvider>,
    interaction: Arc<dyn oxideterm_security_key::SecurityKeyInteraction>,
}

fn verify_security_key_signature(
    key: &PrivateKey,
    required_flags: u8,
    mut to_sign: Vec<u8>,
    response: oxideterm_security_key::SecurityKeyResponse,
) -> Result<Vec<u8>, oxideterm_security_key::SecurityKeyError> {
    use oxideterm_security_key::{SecurityKeyError, SecurityKeyResponse};
    use signature::Verifier;
    let SecurityKeyResponse::Signature {
        signature,
        flags,
        counter,
    } = response
    else {
        return Err(SecurityKeyError::Incompatible);
    };
    if flags & required_flags & 0x05 != required_flags & 0x05 {
        return Err(SecurityKeyError::InvalidSignature);
    }
    let mut verification_bytes = signature.clone();
    verification_bytes.push(flags);
    verification_bytes.extend(counter.to_be_bytes());
    let verification = russh::keys::ssh_key::Signature::new(key.algorithm(), verification_bytes)
        .map_err(|_| SecurityKeyError::InvalidSignature)?;
    Verifier::verify(key.public_key(), &to_sign, &verification)
        .map_err(|_| SecurityKeyError::InvalidSignature)?;
    // Both SK algorithms put flags and counter after the signature
    // string; encode that wire contract independently of key-library helpers.
    let mut wire = Vec::new();
    key.algorithm()
        .encode(&mut wire)
        .map_err(|_| SecurityKeyError::InvalidSignature)?;
    signature
        .encode(&mut wire)
        .map_err(|_| SecurityKeyError::InvalidSignature)?;
    flags
        .encode(&mut wire)
        .map_err(|_| SecurityKeyError::InvalidSignature)?;
    counter
        .encode(&mut wire)
        .map_err(|_| SecurityKeyError::InvalidSignature)?;
    wire.encode(&mut to_sign)
        .map_err(|_| SecurityKeyError::InvalidSignature)?;
    Ok(to_sign)
}

impl SecurityKeySigner {
    async fn new(
        key: Arc<PrivateKey>,
        handler: Option<&dyn SshPromptHandler>,
    ) -> Result<Self, SshTransportError> {
        let handler = handler.ok_or_else(|| {
            SshTransportError::AuthenticationFailed(
                "Security-key signing requires a prompt handler".into(),
            )
        })?;
        let provider = handler
            .security_key_provider()
            .await
            .map_err(SshTransportError::AuthenticationFailed)?;
        let interaction = handler.security_key_interaction().ok_or_else(|| {
            SshTransportError::AuthenticationFailed(
                "Security-key interaction is unavailable".into(),
            )
        })?;
        Ok(Self {
            key,
            provider,
            interaction,
        })
    }
}

impl RusshSigner for SecurityKeySigner {
    type Error = LocalSignerError;

    fn auth_sign(
        &mut self,
        _key: &AgentIdentity,
        _hash_alg: Option<HashAlg>,
        to_sign: Vec<u8>,
    ) -> impl Future<Output = Result<Vec<u8>, Self::Error>> + Send {
        async move {
            use oxideterm_security_key::SecurityKeyRequest;
            let (application, handle, required_flags) = match self.key.key_data() {
                KeypairData::SkEd25519(key) => {
                    (key.public().application(), key.key_handle(), key.flags())
                }
                KeypairData::SkEcdsaSha2NistP256(key) => {
                    (key.public().application(), key.key_handle(), key.flags())
                }
                _ => {
                    return Err(LocalSignerError::Sign(
                        "Unsupported security-key algorithm".into(),
                    ));
                }
            };
            let request = SecurityKeyRequest::Sign {
                algorithm: self.key.algorithm().to_string(),
                application: application.to_string(),
                key_handle: Zeroizing::new(handle.to_vec()),
                flags: required_flags,
                challenge: to_sign.clone(),
            };
            let response = self
                .provider
                .sign(request, Arc::clone(&self.interaction))
                .await;
            let result = response.and_then(|response| {
                verify_security_key_signature(&self.key, required_flags, to_sign, response)
            });
            match result {
                Ok(signed) => Ok(signed),
                Err(error) => Err(LocalSignerError::Sign(
                    self.interaction.error_message(error).await,
                )),
            }
        }
    }
}

#[cfg(test)]
mod security_key_tests {
    use super::*;
    use oxideterm_security_key::{SecurityKeyError, SecurityKeyResponse};
    use russh::keys::ssh_key::{EcdsaCurve, LineEnding, private, public};
    use sha2::{Digest, Sha256};
    use ssh_encoding::Decode;

    fn hardware_key(ordinary: &PrivateKey, application: &str) -> PrivateKey {
        let data = match ordinary.public_key().key_data() {
            public::KeyData::Ed25519(public) => KeypairData::SkEd25519(
                private::SkEd25519::new(
                    public::SkEd25519::new(*public, application),
                    5,
                    vec![1, 2, 3],
                )
                .unwrap(),
            ),
            public::KeyData::Ecdsa(public::EcdsaPublicKey::NistP256(point)) => {
                KeypairData::SkEcdsaSha2NistP256(
                    private::SkEcdsaSha2NistP256::new(
                        public::SkEcdsaSha2NistP256::new(*point, application),
                        5,
                        vec![1, 2, 3],
                    )
                    .unwrap(),
                )
            }
            _ => panic!("unsupported fixture key"),
        };
        PrivateKey::new(data, "software fixture").unwrap()
    }

    fn signed_response(
        ordinary: &PrivateKey,
        message: &[u8],
        flags: u8,
        counter: u32,
    ) -> SecurityKeyResponse {
        // FIDO's signed bytes are an external protocol contract, independently
        // constructed here with an ordinary software key, not a hardware mock.
        let mut signed = Sha256::digest(b"ssh:fixture").to_vec();
        signed.push(flags);
        signed.extend(counter.to_be_bytes());
        signed.extend(Sha256::digest(message));
        let signature = SignatureSigner::try_sign(ordinary, &signed).unwrap();
        SecurityKeyResponse::Signature {
            signature: signature.as_bytes().to_vec(),
            flags,
            counter,
        }
    }

    #[test]
    fn security_key_signatures_bind_key_application_challenge_and_flags_and_use_ssh_wire_layout() {
        let mut rng = rand10::rand_core::UnwrapErr(rand10::rngs::SysRng);
        for algorithm in [
            Algorithm::Ed25519,
            Algorithm::Ecdsa {
                curve: EcdsaCurve::NistP256,
            },
        ] {
            let ordinary = PrivateKey::random(&mut rng, algorithm).unwrap();
            let key = hardware_key(&ordinary, "ssh:fixture");
            let challenge = b"SSH authentication challenge";
            let text = key.to_openssh(LineEnding::LF).unwrap();
            let decoded = decode_private_key_for_auth(text.as_str(), None).unwrap();
            assert_eq!(decoded.algorithm(), key.algorithm());
            assert_eq!(decoded.public_key(), key.public_key());
            let signed = verify_security_key_signature(
                &key,
                5,
                challenge.to_vec(),
                signed_response(&ordinary, challenge, 5, 42),
            )
            .unwrap();
            assert_eq!(&signed[..challenge.len()], challenge);
            let mut remainder = &signed[challenge.len()..];
            let wire = Vec::<u8>::decode(&mut remainder).unwrap();
            assert!(remainder.is_empty());
            let mut wire = wire.as_slice();
            assert_eq!(Algorithm::decode(&mut wire).unwrap(), key.algorithm());
            let expected = match signed_response(&ordinary, challenge, 5, 42) {
                SecurityKeyResponse::Signature { signature, .. } => signature,
                _ => unreachable!(),
            };
            // ECDSA may use randomized signing; compare structure rather than
            // two signatures from separate operations.
            let raw = Vec::<u8>::decode(&mut wire).unwrap();
            if matches!(key.algorithm(), Algorithm::SkEd25519) {
                assert_eq!(raw, expected);
            } else {
                let mut components = raw.as_slice();
                for _ in 0..2 {
                    let component = russh::keys::ssh_key::Mpint::decode(&mut components).unwrap();
                    assert!(component.as_positive_bytes().unwrap().len() <= 32);
                }
                assert!(components.is_empty());
            }
            assert_eq!(u8::decode(&mut wire).unwrap(), 5);
            assert_eq!(u32::decode(&mut wire).unwrap(), 42);
            assert!(wire.is_empty());
            for (candidate, response) in [
                (
                    challenge.to_vec(),
                    signed_response(&ordinary, challenge, 1, 42),
                ),
                (
                    b"another authentication".to_vec(),
                    signed_response(&ordinary, challenge, 5, 42),
                ),
            ] {
                assert_eq!(
                    verify_security_key_signature(&key, 5, candidate, response).unwrap_err(),
                    SecurityKeyError::InvalidSignature
                );
            }
            let other_application = hardware_key(&ordinary, "ssh:other");
            assert_eq!(
                verify_security_key_signature(
                    &other_application,
                    5,
                    challenge.to_vec(),
                    signed_response(&ordinary, challenge, 5, 42)
                )
                .unwrap_err(),
                SecurityKeyError::InvalidSignature
            );
        }
    }
}
