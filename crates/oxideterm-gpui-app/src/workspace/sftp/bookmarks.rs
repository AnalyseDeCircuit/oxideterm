use super::*;
use oxideterm_sftp::bookmarks::sftp_bookmark_node_remote_key;

impl WorkspaceApp {
    pub(in crate::workspace::sftp) fn sftp_bookmark_write_result(
        &self,
        result: std::io::Result<bool>,
        cx: &App,
    ) -> bool {
        match result {
            Ok(changed) => changed,
            Err(error) => {
                self.push_sftp_toast(
                    self.i18n.t("sftp.bookmarks.save_failed"),
                    Some(error.to_string()),
                    TerminalNoticeVariant::Error,
                    cx,
                );
                // A failed write retains the edited in-memory entry as before.
                true
            }
        }
    }

    pub(in crate::workspace::sftp) fn sftp_bookmark_key_for_remote(
        &self,
        remote_id: &SftpRemoteId,
    ) -> String {
        match remote_id {
            SftpRemoteId::Node(node_id) => match self.ssh_nodes.get(node_id) {
                Some(node) => sftp_bookmark_node_remote_key(
                    node.saved_connection_id.as_deref(),
                    &node.endpoint.username,
                    &node.endpoint.host,
                    node.endpoint.port,
                ),
                // The node projection is normally present; keep the tab usable
                // if it is being torn down while a menu is open.
                None => remote_id.storage_key(),
            },
            SftpRemoteId::Standalone(_) | SftpRemoteId::Ftp(_) => remote_id.storage_key(),
        }
    }
}
