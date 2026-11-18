use gpui::Context;
use oxideterm_gpui_ai::acp;
use oxideterm_gpui_terminal::SharedTerminalSession;
use oxideterm_sftp::TextDiffLineKind;

pub(in crate::workspace) type AcpWorkspaceEntity = acp::AcpWorkspaceEntity<SharedTerminalSession>;

pub(in crate::workspace) fn new_acp_workspace(
    task_runtime: std::sync::Arc<tokio::runtime::Runtime>,
    cx: &mut Context<AcpWorkspaceEntity>,
) -> AcpWorkspaceEntity {
    AcpWorkspaceEntity::new(task_runtime, |session| session.lock().shutdown(), cx)
}

pub(in crate::workspace) fn acp_file_write_preview(
    existing: &str,
    proposed: &str,
) -> zeroize::Zeroizing<String> {
    const MAX_PREVIEW_LINES: usize = 400;
    const MAX_PREVIEW_BYTES: usize = 64 * 1024;

    let mut preview = String::from("--- existing\n+++ proposed\n");
    let lines = oxideterm_sftp::compute_text_diff(existing, proposed);
    let truncated_lines = lines.len() > MAX_PREVIEW_LINES;
    for line in lines.into_iter().take(MAX_PREVIEW_LINES) {
        let prefix = match line.kind {
            TextDiffLineKind::Unchanged => ' ',
            TextDiffLineKind::Added => '+',
            TextDiffLineKind::Removed => '-',
        };
        preview.push(prefix);
        preview.push_str(&line.content);
        preview.push('\n');
        if preview.len() >= MAX_PREVIEW_BYTES {
            break;
        }
    }
    if truncated_lines || preview.len() >= MAX_PREVIEW_BYTES {
        let mut boundary = preview.len().min(MAX_PREVIEW_BYTES);
        while !preview.is_char_boundary(boundary) {
            boundary = boundary.saturating_sub(1);
        }
        preview.truncate(boundary);
        preview.push_str("\n…");
    }
    zeroize::Zeroizing::new(preview)
}

impl super::WorkspaceApp {
    pub(in crate::workspace) fn acp_terminal_bridge(
        cx: &Context<Self>,
    ) -> oxideterm_gpui_ai::conversation::acp_terminal::AcpTerminalBridge<SharedTerminalSession>
    {
        use oxideterm_gpui_ai::conversation::acp_terminal::{AcpTerminalBridge, AcpTerminalStatus};
        use oxideterm_terminal::{LocalPtyConfig, ShellInfo, TerminalLifecycle};

        fn status(lifecycle: TerminalLifecycle) -> AcpTerminalStatus {
            match lifecycle {
                TerminalLifecycle::Running => AcpTerminalStatus::Running,
                TerminalLifecycle::Exited(exit_code) => AcpTerminalStatus::Exited(exit_code),
                TerminalLifecycle::Closed => AcpTerminalStatus::Closed,
            }
        }

        let owner = cx.weak_entity();
        AcpTerminalBridge {
            launch: Box::new(move |mut spec, cwd, window, cx| {
                let title = std::path::Path::new(&spec.command)
                    .file_name()
                    .and_then(|name| name.to_str())
                    .filter(|name| !name.is_empty())
                    .unwrap_or("ACP")
                    .to_string();
                let config = LocalPtyConfig {
                    command: None,
                    shell: Some(
                        ShellInfo::new(
                            title.clone(),
                            title.clone(),
                            std::path::PathBuf::from(std::mem::take(&mut spec.command)),
                        )
                        .with_args(std::mem::take(&mut spec.args)),
                    ),
                    cwd: Some(cwd),
                    env: std::mem::take(&mut spec.env),
                    remove_env: Vec::new(),
                    load_profile: false,
                    current_directory_shell_integration: false,
                    oh_my_posh_enabled: false,
                    oh_my_posh_theme: None,
                };
                owner
                    .update(cx, |app, cx| {
                        app.create_local_terminal_tab_with_owned_session(config, title, window, cx)
                    })
                    .ok()?
                    .ok()
                    .map(|(_, session)| session)
            }),
            read_output: |session| {
                let mut session = session.lock();
                session.read_pending();
                (session.buffer_text(), status(session.lifecycle()))
            },
            status: |session| status(session.lock().lifecycle()),
            stop: |session| session.lock().shutdown(),
        }
    }
}
