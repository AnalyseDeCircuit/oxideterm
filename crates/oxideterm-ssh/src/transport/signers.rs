type NativeAgentClient = AgentClient<Box<dyn AgentStream + Send + Unpin + 'static>>;

/// Send-safe wrapper around russh's agent client.
///
/// Tauri keeps the same wrapper because russh's blanket `Signer` impl for
/// `AgentClient` can produce a future whose borrowed `AgentIdentity` lifetime
/// is not general enough once the SSH connect flow is spawned onto a Send
/// runtime. Native terminal startup has the same Send boundary, so keep the
/// owned-key clone here instead of passing `AgentClient` directly.
struct AgentSigner<'a> {
    agent: &'a mut NativeAgentClient,
}

impl RusshSigner for AgentSigner<'_> {
    type Error = AgentAuthError;

    fn auth_sign(
        &mut self,
        key: &AgentIdentity,
        hash_alg: Option<HashAlg>,
        to_sign: Vec<u8>,
    ) -> impl std::future::Future<Output = Result<Vec<u8>, Self::Error>> + Send {
        let key_owned = key.clone();
        async move {
            self.agent
                .sign_request(&key_owned, hash_alg, to_sign)
                .await
                .map_err(Into::into)
        }
    }
}

#[derive(Debug, thiserror::Error)]
enum LocalSignerError {
    #[error(transparent)]
    Send(#[from] russh::SendError),
    #[error("{0}")]
    Sign(String),
}

struct LocalKeySigner {
    key: Arc<PrivateKey>,
}

impl LocalKeySigner {
    fn new(key: Arc<PrivateKey>) -> Self {
        Self { key }
    }
}

impl RusshSigner for LocalKeySigner {
    type Error = LocalSignerError;

    fn auth_sign(
        &mut self,
        _key: &AgentIdentity,
        hash_alg: Option<HashAlg>,
        to_sign: Vec<u8>,
    ) -> impl Future<Output = Result<Vec<u8>, Self::Error>> + Send {
        let key = Arc::clone(&self.key);
        async move { sign_auth_payload_with_hash_alg(key.as_ref(), hash_alg, to_sign) }
    }
}
