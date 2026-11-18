use gpui::Context;
use oxideterm_gpui_ai::acp;
use oxideterm_gpui_terminal::SharedTerminalSession;
use oxideterm_sftp::TextDiffLineKind;

use super::sidebar::handle_acp_application_tool_call;

pub(in crate::workspace) type AcpWorkspaceEntity = acp::AcpWorkspaceEntity<SharedTerminalSession>;

pub(in crate::workspace) fn new_acp_workspace(
    task_runtime: std::sync::Arc<tokio::runtime::Runtime>,
    cx: &mut Context<AcpWorkspaceEntity>,
) -> AcpWorkspaceEntity {
    AcpWorkspaceEntity::new(
        task_runtime,
        |session| session.lock().shutdown(),
        |call, turn| Box::pin(handle_acp_application_tool_call(call, turn)),
        cx,
    )
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
