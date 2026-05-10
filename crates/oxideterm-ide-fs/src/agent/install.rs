fn arch_to_target(arch: &str) -> Result<&'static str, AgentError> {
    match arch {
        "x86_64" | "amd64" => Ok("x86_64-linux-musl"),
        "aarch64" | "arm64" => Ok("aarch64-linux-musl"),
        other => Err(AgentError::UnsupportedArch(other.to_string())),
    }
}

async fn probe_remote_install(
    handle: &SshConnectionHandle,
    remote_path: &str,
) -> RemoteAgentInstallState {
    let command = format!(
        "'{}' --version 2>/dev/null || echo 'NOT_FOUND'",
        shell_single_quote(remote_path)
    );
    match handle
        .run_command(&command, Duration::from_secs(5), 2048)
        .await
    {
        Ok(output) => parse_remote_version_output(output.trim()),
        Err(_) => RemoteAgentInstallState::Missing,
    }
}

fn parse_remote_version_output(output: &str) -> RemoteAgentInstallState {
    let trimmed = output.trim();
    if trimmed.is_empty() || trimmed.contains("NOT_FOUND") {
        return RemoteAgentInstallState::Missing;
    }

    let mut parts = trimmed.split_whitespace();
    let _binary_name = parts.next();
    let version = parts.next().unwrap_or(trimmed).to_string();
    let mut compatibility_version = LEGACY_AGENT_COMPATIBILITY_VERSION;
    let mut saw_compat_marker = false;
    while let Some(part) = parts.next() {
        if part == "compat" {
            saw_compat_marker = true;
            compatibility_version = parts
                .next()
                .and_then(|raw| raw.parse::<u32>().ok())
                .unwrap_or(INVALID_AGENT_COMPATIBILITY_VERSION);
            break;
        }
    }
    if !saw_compat_marker {
        compatibility_version = LEGACY_AGENT_COMPATIBILITY_VERSION;
    }

    if compatibility_version == CURRENT_AGENT_COMPATIBILITY_VERSION {
        RemoteAgentInstallState::Current
    } else {
        RemoteAgentInstallState::Incompatible(RemoteAgentVersionInfo {
            version,
            compatibility_version,
        })
    }
}

fn resolve_agent_binary(target: &str) -> Result<PathBuf, AgentError> {
    let file_name = format!("oxideterm-agent-{target}");
    for dir in agent_resource_dirs() {
        let candidate = dir.join(&file_name);
        if candidate.exists() {
            return Ok(candidate);
        }
    }
    Err(AgentError::BinaryNotFound(format!(
        "agents/{file_name}; set OXIDETERM_AGENT_DIR or package it in app resources"
    )))
}

fn agent_resource_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    if let Some(dir) = std::env::var_os("OXIDETERM_AGENT_DIR") {
        dirs.push(PathBuf::from(dir));
    }
    if let Ok(exe) = std::env::current_exe()
        && let Some(exe_dir) = exe.parent()
    {
        // Tauri resolves from app resources. Native keeps the same contract:
        // package the prebuilt remote-agent binaries under `agents/`, and only
        // use source-tree locations as developer fallbacks.
        dirs.push(exe_dir.join("../Resources/agents"));
        dirs.push(exe_dir.join("resources/agents"));
        dirs.push(exe_dir.join("agents"));
    }
    if let Ok(cwd) = std::env::current_dir() {
        dirs.push(cwd.join("crates/oxideterm-gpui-app/resources/agents"));
        dirs.push(cwd.join("tauri版本代码/src-tauri/agents"));
    }
    dirs
}

async fn upload_agent(
    handle: &SshConnectionHandle,
    router: &NodeRouter,
    node_id: &NodeId,
    remote_path: &str,
    binary_path: &PathBuf,
) -> Result<(), AgentError> {
    let remote_dir = remote_path
        .rsplit_once('/')
        .map(|(dir, _)| dir)
        .ok_or_else(|| AgentError::Ssh(format!("Invalid remote agent path: {remote_path}")))?;
    handle
        .run_command(
            &format!("mkdir -p -- '{}'", shell_single_quote(remote_dir)),
            Duration::from_secs(30),
            2048,
        )
        .await
        .map_err(|error| AgentError::Ssh(error.to_string()))?;

    let sftp = router.acquire_sftp(node_id).await?;
    let sftp = sftp.lock().await;
    let binary = tokio::fs::read(binary_path)
        .await
        .map_err(|error| AgentError::LocalIo(error.to_string()))?;
    sftp.write_content(remote_path, &binary).await?;
    handle
        .run_command(
            &format!("chmod +x -- '{}'", shell_single_quote(remote_path)),
            Duration::from_secs(30),
            2048,
        )
        .await
        .map_err(|error| AgentError::Ssh(error.to_string()))?;
    Ok(())
}

fn shell_single_quote(value: &str) -> String {
    value.replace('\'', "'\\''")
}

async fn handshake_agent(transport: &AgentTransport) -> Result<SysInfoResult, AgentError> {
    transport
        .call_with_timeout("sys/ping", serde_json::json!({}), 10)
        .await
        .map_err(|error| AgentError::Handshake(format!("Ping failed: {error}")))?;
    let info_value = transport
        .call_with_timeout("sys/info", serde_json::json!({}), 10)
        .await
        .map_err(|error| AgentError::Handshake(format!("sys/info failed: {error}")))?;
    let info: SysInfoResult = serde_json::from_value(info_value)
        .map_err(|error| AgentError::Handshake(format!("Invalid sys/info response: {error}")))?;
    if info.compatibility_version != CURRENT_AGENT_COMPATIBILITY_VERSION {
        return Err(AgentError::Handshake(format!(
            "Agent compatibility mismatch: got {}, expected {}",
            info.compatibility_version, CURRENT_AGENT_COMPATIBILITY_VERSION
        )));
    }
    Ok(info)
}
