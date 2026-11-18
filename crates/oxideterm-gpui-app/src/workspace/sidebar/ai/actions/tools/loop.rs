/// Resolves the same local ACP session directory for prompts and pre-prompt discovery.
pub(in crate::workspace) fn acp_session_cwd_from_agent(
    agent: &oxideterm_settings::AcpAgentConfig,
) -> std::path::PathBuf {
    std::env::current_dir().unwrap_or_else(|_| {
        agent
            .cwd
            .as_deref()
            .map(std::path::PathBuf::from)
            .unwrap_or_else(|| std::path::PathBuf::from("."))
    })
}

