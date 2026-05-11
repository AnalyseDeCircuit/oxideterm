fn should_retry_password_auth(result: &client::AuthResult) -> bool {
    matches!(
        result,
        client::AuthResult::Failure {
            partial_success: false,
            ..
        }
    )
}

async fn try_password_as_keyboard_interactive(
    handle: &mut client::Handle<NativeClientHandler>,
    config: &SshConfig,
    password: &str,
    password_result: &client::AuthResult,
    prompt_handler: Option<&dyn SshPromptHandler>,
) -> Result<bool, SshTransportError> {
    let client::AuthResult::Failure {
        partial_success: false,
        remaining_methods,
    } = password_result
    else {
        return Ok(false);
    };
    if !remaining_methods.contains(&MethodKind::KeyboardInteractive)
        || remaining_methods.contains(&MethodKind::Password)
    {
        return Ok(false);
    }

    let mut password_prompt_consumed = false;
    let mut response = tokio::time::timeout(
        PASSWORD_AUTH_TIMEOUT,
        handle.authenticate_keyboard_interactive_start(config.username.clone(), None::<String>),
    )
    .await
    .map_err(|_| {
        SshTransportError::AuthenticationFailed(
            "keyboard-interactive password fallback timed out".to_string(),
        )
    })?
    .map_err(|error| {
        SshTransportError::AuthenticationFailed(format!(
            "keyboard-interactive password fallback failed: {error}"
        ))
    })?;

    for _ in 0..MAX_PASSWORD_KBI_FALLBACK_ROUNDS {
        match response {
            client::KeyboardInteractiveAuthResponse::Success => return Ok(true),
            client::KeyboardInteractiveAuthResponse::Failure { .. } => return Ok(false),
            client::KeyboardInteractiveAuthResponse::InfoRequest {
                name,
                instructions,
                prompts,
            } => {
                let replies = if prompts.is_empty() {
                    Vec::new()
                } else if !password_prompt_consumed
                    && prompts.len() == 1
                    && !prompts[0].echo
                    && prompt_looks_like_password(&prompts[0].prompt)
                {
                    password_prompt_consumed = true;
                    vec![password.to_string()]
                } else {
                    let Some(prompt_handler) = prompt_handler else {
                        return Ok(false);
                    };
                    return continue_keyboard_interactive_flow(
                        handle,
                        prompt_handler,
                        client::KeyboardInteractiveAuthResponse::InfoRequest {
                            name,
                            instructions,
                            prompts,
                        },
                        false,
                    )
                    .await;
                };
                response = tokio::time::timeout(
                    PASSWORD_AUTH_TIMEOUT,
                    handle.authenticate_keyboard_interactive_respond(replies),
                )
                .await
                .map_err(|_| {
                    SshTransportError::AuthenticationFailed(
                        "keyboard-interactive password fallback response timed out".to_string(),
                    )
                })?
                .map_err(|error| {
                    SshTransportError::AuthenticationFailed(format!(
                        "keyboard-interactive password fallback response failed: {error}"
                    ))
                })?;
            }
        }
    }
    Ok(false)
}

async fn authenticate_keyboard_interactive(
    handle: &mut client::Handle<NativeClientHandler>,
    username: &str,
    prompt_handler: Option<&dyn SshPromptHandler>,
) -> Result<client::AuthResult, SshTransportError> {
    let Some(prompt_handler) = prompt_handler else {
        return Err(SshTransportError::UnsupportedAuth(
            "keyboard-interactive requires a native prompt flow",
        ));
    };
    let response = tokio::time::timeout(
        PASSWORD_AUTH_TIMEOUT,
        handle.authenticate_keyboard_interactive_start(username, None::<String>),
    )
    .await
    .map_err(|_| {
        SshTransportError::AuthenticationFailed(
            "keyboard-interactive authentication timed out".to_string(),
        )
    })?
    .map_err(|error| {
        SshTransportError::AuthenticationFailed(format!(
            "keyboard-interactive authentication start failed: {error}"
        ))
    })?;
    let success =
        continue_keyboard_interactive_flow(handle, prompt_handler, response, false).await?;
    Ok(if success {
        client::AuthResult::Success
    } else {
        client::AuthResult::Failure {
            remaining_methods: russh::MethodSet::empty(),
            partial_success: false,
        }
    })
}

async fn try_keyboard_interactive_chain(
    handle: &mut client::Handle<NativeClientHandler>,
    username: &str,
    auth_result: &client::AuthResult,
    prompt_handler: Option<&dyn SshPromptHandler>,
) -> Result<bool, SshTransportError> {
    let client::AuthResult::Failure {
        partial_success: true,
        remaining_methods,
    } = auth_result
    else {
        return Ok(false);
    };
    if !remaining_methods.contains(&MethodKind::KeyboardInteractive) {
        return Ok(false);
    }
    let Some(prompt_handler) = prompt_handler else {
        return Ok(false);
    };
    let response = handle
        .authenticate_keyboard_interactive_start(username, None::<String>)
        .await
        .map_err(|error| {
            SshTransportError::AuthenticationFailed(format!(
                "keyboard-interactive chained authentication start failed: {error}"
            ))
        })?;
    continue_keyboard_interactive_flow(handle, prompt_handler, response, true).await
}

async fn continue_keyboard_interactive_flow(
    handle: &mut client::Handle<NativeClientHandler>,
    prompt_handler: &dyn SshPromptHandler,
    mut response: client::KeyboardInteractiveAuthResponse,
    chained: bool,
) -> Result<bool, SshTransportError> {
    loop {
        match response {
            client::KeyboardInteractiveAuthResponse::Success => return Ok(true),
            client::KeyboardInteractiveAuthResponse::Failure { .. } => return Ok(false),
            client::KeyboardInteractiveAuthResponse::InfoRequest {
                name,
                instructions,
                prompts,
            } => {
                let request = KeyboardInteractivePromptRequest {
                    flow_id: uuid::Uuid::new_v4().to_string(),
                    name,
                    instructions,
                    prompts: prompts
                        .into_iter()
                        .map(|prompt| KeyboardInteractivePrompt {
                            prompt: prompt.prompt,
                            echo: prompt.echo,
                        })
                        .collect(),
                    chained,
                };
                let replies = tokio::time::timeout(
                    KBI_USER_PROMPT_TIMEOUT,
                    prompt_handler.keyboard_interactive(request),
                )
                .await
                .map_err(|_| {
                    SshTransportError::AuthenticationFailed(SshPromptError::Timeout.to_string())
                })?
                .map_err(|error| SshTransportError::AuthenticationFailed(error.to_string()))?;
                response = tokio::time::timeout(
                    PASSWORD_AUTH_TIMEOUT,
                    handle.authenticate_keyboard_interactive_respond(replies),
                )
                .await
                .map_err(|_| {
                    SshTransportError::AuthenticationFailed(
                        "keyboard-interactive response timed out".to_string(),
                    )
                })?
                .map_err(|error| {
                    SshTransportError::AuthenticationFailed(format!(
                        "keyboard-interactive response failed: {error}"
                    ))
                })?;
            }
        }
    }
}

fn prompt_looks_like_password(prompt: &str) -> bool {
    let normalized = prompt.trim().to_ascii_lowercase();
    normalized.contains("password") || prompt.contains("密码")
}

fn authentication_failure_message(result: &client::AuthResult) -> String {
    match result {
        client::AuthResult::Success => "authentication succeeded".to_string(),
        client::AuthResult::Failure {
            remaining_methods,
            partial_success,
        } => {
            let methods = remaining_methods
                .iter()
                .map(|method| String::from(<&str>::from(method)))
                .collect::<Vec<_>>()
                .join(", ");
            if methods.is_empty() {
                format!("rejected by server; partial_success={partial_success}")
            } else {
                format!(
                    "rejected by server; remaining methods: {methods}; partial_success={partial_success}"
                )
            }
        }
    }
}

fn load_private_key_material(
    key_path: &str,
    passphrase: Option<&str>,
) -> Result<Arc<PrivateKey>, SshTransportError> {
    let key = if key_path.trim().is_empty() {
        load_first_available_default_key(passphrase)?
    } else {
        let key_path = expand_tilde_path(key_path);
        load_secret_key(&key_path, passphrase)
            .map_err(|error| SshTransportError::AuthenticationFailed(error.to_string()))?
    };
    Ok(Arc::new(key))
}

fn load_certificate_auth_material(
    key_path: &str,
    cert_path: &str,
    passphrase: Option<&str>,
) -> Result<(Arc<PrivateKey>, Certificate), SshTransportError> {
    let key = load_private_key_material(key_path, passphrase)?;
    let cert_path = expand_tilde_path(cert_path);
    let cert = load_openssh_certificate(&cert_path)
        .map_err(|error| SshTransportError::AuthenticationFailed(error.to_string()))?;
    Ok((key, cert))
}

async fn resolve_server_rsa_preference(
    handle: &client::Handle<NativeClientHandler>,
) -> Option<Option<HashAlg>> {
    handle.best_supported_rsa_hash().await.ok().flatten()
}

fn auth_algorithm_attempt_order(
    is_rsa: bool,
    server_preference: Option<Option<HashAlg>>,
) -> Vec<Option<HashAlg>> {
    if !is_rsa {
        return vec![None];
    }

    match server_preference {
        Some(None) => vec![None],
        Some(Some(preferred_hash)) => {
            let mut algorithms = vec![Some(preferred_hash)];
            algorithms.extend(
                RSA_AUTH_ALGORITHMS
                    .iter()
                    .copied()
                    .filter(|candidate| *candidate != Some(preferred_hash)),
            );
            algorithms
        }
        None => RSA_AUTH_ALGORITHMS.to_vec(),
    }
}

fn server_allows_more_publickey_attempts(result: &client::AuthResult) -> bool {
    matches!(
        result,
        client::AuthResult::Failure {
            remaining_methods,
            ..
        } if remaining_methods.contains(&MethodKind::PublicKey)
    )
}

async fn authenticate_publickey_best_algo(
    handle: &mut client::Handle<NativeClientHandler>,
    username: &str,
    key: Arc<PrivateKey>,
) -> Result<client::AuthResult, SshTransportError> {
    let algorithms = auth_algorithm_attempt_order(
        matches!(key.algorithm(), Algorithm::Rsa { .. }),
        resolve_server_rsa_preference(handle).await,
    );
    let mut last_result = None;

    for hash_alg in algorithms {
        let result = handle
            .authenticate_publickey(
                username,
                PrivateKeyWithHashAlg::new(Arc::clone(&key), hash_alg),
            )
            .await
            .map_err(|error| SshTransportError::AuthenticationFailed(error.to_string()))?;
        if result.success() || !server_allows_more_publickey_attempts(&result) {
            return Ok(result);
        }
        last_result = Some(result);
    }

    Ok(last_result.unwrap_or_else(|| client::AuthResult::Failure {
        remaining_methods: russh::MethodSet::empty(),
        partial_success: false,
    }))
}

async fn authenticate_certificate_best_algo(
    handle: &mut client::Handle<NativeClientHandler>,
    username: &str,
    key: Arc<PrivateKey>,
    cert: Certificate,
) -> Result<client::AuthResult, SshTransportError> {
    let algorithms = auth_algorithm_attempt_order(
        matches!(cert.algorithm(), Algorithm::Rsa { .. }),
        resolve_server_rsa_preference(handle).await,
    );
    let mut signer = LocalKeySigner::new(key);
    let mut last_result = None;

    for hash_alg in algorithms {
        let result = handle
            .authenticate_certificate_with(username, cert.clone(), hash_alg, &mut signer)
            .await
            .map_err(|error| {
                SshTransportError::AuthenticationFailed(format!(
                    "certificate authentication failed: {error}"
                ))
            })?;
        if result.success() || !server_allows_more_publickey_attempts(&result) {
            return Ok(result);
        }
        last_result = Some(result);
    }

    Ok(last_result.unwrap_or_else(|| client::AuthResult::Failure {
        remaining_methods: russh::MethodSet::empty(),
        partial_success: false,
    }))
}

fn sign_auth_payload_with_hash_alg(
    key: &PrivateKey,
    hash_alg: Option<HashAlg>,
    mut data: Vec<u8>,
) -> Result<Vec<u8>, LocalSignerError> {
    let signature = match key.key_data() {
        KeypairData::Rsa(rsa_keypair) => {
            SignatureSigner::try_sign(&(rsa_keypair, hash_alg), data.as_slice())
                .map_err(|error| LocalSignerError::Sign(error.to_string()))?
        }
        keypair => SignatureSigner::try_sign(keypair, data.as_slice())
            .map_err(|error| LocalSignerError::Sign(error.to_string()))?,
    };

    let mut encoded_signature = Vec::new();
    signature
        .encode(&mut encoded_signature)
        .map_err(|error| LocalSignerError::Sign(error.to_string()))?;
    encoded_signature
        .encode(&mut data)
        .map_err(|error| LocalSignerError::Sign(error.to_string()))?;
    Ok(data)
}

async fn authenticate_agent(
    handle: &mut client::Handle<NativeClientHandler>,
    config: &SshConfig,
) -> Result<client::AuthResult, SshTransportError> {
    let mut agent = connect_agent_client()
        .await
        .map_err(|error| SshTransportError::AuthenticationFailed(error.to_string()))?;
    let identities = agent
        .request_identities()
        .await
        .map_err(|error| SshTransportError::AuthenticationFailed(error.to_string()))?;
    if identities.is_empty() {
        return Err(SshTransportError::AuthenticationFailed(
            "SSH agent has no identities".to_string(),
        ));
    }

    let server_rsa_preference = resolve_server_rsa_preference(handle).await;
    let mut last_error = None;
    let mut publickey_exhausted = false;
    for identity in identities {
        let public_key = identity.public_key().into_owned();
        let algorithms = auth_algorithm_attempt_order(
            matches!(public_key.algorithm(), Algorithm::Rsa { .. }),
            server_rsa_preference,
        );
        for hash_alg in algorithms {
            match handle
                .authenticate_publickey_with(
                    config.username.clone(),
                    public_key.clone(),
                    hash_alg,
                    &mut AgentSigner { agent: &mut agent },
                )
                .await
            {
                Ok(result) if result.success() => return Ok(client::AuthResult::Success),
                Ok(result) => {
                    if !server_allows_more_publickey_attempts(&result) {
                        publickey_exhausted = true;
                        break;
                    }
                }
                Err(AgentAuthError::Send(send)) => {
                    return Err(SshTransportError::AuthenticationFailed(send.to_string()));
                }
                Err(AgentAuthError::Key(key_error)) => {
                    last_error = Some(key_error.to_string());
                }
            }
        }

        if publickey_exhausted {
            break;
        }
    }

    Err(SshTransportError::AuthenticationFailed(format!(
        "No agent key was accepted by the server{}",
        last_error
            .map(|error| format!(". Last error: {error}"))
            .unwrap_or_default()
    )))
}

async fn connect_agent_client() -> Result<NativeAgentClient, String> {
    #[cfg(unix)]
    {
        AgentClient::connect_env()
            .await
            .map(|agent| agent.dynamic())
            .map_err(|error| {
                format!(
                    "Failed to connect to SSH Agent: {error}. Make sure SSH_AUTH_SOCK is set and ssh-agent is running."
                )
            })
    }

    #[cfg(windows)]
    {
        AgentClient::connect_named_pipe(r"\\.\pipe\openssh-ssh-agent")
            .await
            .map(|agent| agent.dynamic())
            .map_err(|error| {
                format!(
                    "Failed to connect to SSH Agent via named pipe: {error}. Make sure the OpenSSH Authentication Agent service is running."
                )
            })
    }

    #[cfg(not(any(unix, windows)))]
    {
        Err("SSH Agent is not supported on this platform".to_string())
    }
}

async fn handle_agent_forward_channel(channel: Channel<client::Msg>) {
    let agent_stream = match connect_agent_stream().await {
        Ok(stream) => stream,
        Err(_) => {
            let _ = channel.eof().await;
            return;
        }
    };
    relay_agent_forward_channel(channel, agent_stream).await;
}

async fn relay_agent_forward_channel(
    channel: Channel<client::Msg>,
    mut agent_stream: impl tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin,
) {
    let mut channel_stream = channel.into_stream();
    let _ = tokio::io::copy_bidirectional(&mut channel_stream, &mut agent_stream).await;
}

#[cfg(unix)]
async fn connect_agent_stream() -> Result<tokio::net::UnixStream, String> {
    let socket_path =
        std::env::var("SSH_AUTH_SOCK").map_err(|_| "SSH_AUTH_SOCK is not set".to_string())?;
    tokio::net::UnixStream::connect(&socket_path)
        .await
        .map_err(|error| format!("failed to connect to SSH agent socket {socket_path}: {error}"))
}

#[cfg(windows)]
async fn connect_agent_stream() -> Result<tokio::net::windows::named_pipe::NamedPipeClient, String>
{
    use tokio::net::windows::named_pipe::ClientOptions;
    let pipe_name = r"\\.\pipe\openssh-ssh-agent";
    ClientOptions::new()
        .open(pipe_name)
        .map_err(|error| format!("failed to connect to SSH agent named pipe {pipe_name}: {error}"))
}

#[cfg(not(any(unix, windows)))]
async fn connect_agent_stream() -> Result<(), String> {
    Err("SSH agent forwarding is not supported on this platform".to_string())
}
