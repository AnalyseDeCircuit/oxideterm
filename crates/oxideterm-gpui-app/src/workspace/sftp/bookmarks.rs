use super::*;

/// A user-named pair of local and remote directories for one SFTP endpoint.
///
/// `remote_key` is the durable endpoint identity, not the live `NodeId`:
/// `saved:<connection-id>` for saved SSH connections, `endpoint:user@host:port`
/// for direct connections, and the storage key for standalone/FTP endpoints.
#[derive(Clone, Debug, PartialEq, serde::Deserialize, serde::Serialize)]
pub(in crate::workspace) struct SftpPathBookmark {
    pub(in crate::workspace) id: String,
    pub(in crate::workspace) name: String,
    #[serde(rename = "localPath")]
    pub(in crate::workspace) local_path: String,
    #[serde(rename = "remotePath")]
    pub(in crate::workspace) remote_path: String,
    #[serde(rename = "remoteKey")]
    pub(in crate::workspace) remote_key: String,
    #[serde(rename = "createdAt")]
    pub(in crate::workspace) created_at: u64,
}

pub(in crate::workspace::sftp) const SFTP_PATH_BOOKMARKS_FILENAME: &str =
    "oxideterm-sftp-path-bookmarks.json";

pub(in crate::workspace::sftp) fn sftp_path_bookmarks_path(
    settings_path: &std::path::Path,
) -> std::path::PathBuf {
    settings_path
        .parent()
        .unwrap_or(settings_path)
        .join(SFTP_PATH_BOOKMARKS_FILENAME)
}

pub(in crate::workspace::sftp) fn load_sftp_path_bookmarks(
    settings_path: &std::path::Path,
) -> Vec<SftpPathBookmark> {
    let path = sftp_path_bookmarks_path(settings_path);
    let Ok(bytes) = std::fs::read(&path) else {
        return Vec::new();
    };
    match serde_json::from_slice::<Vec<SftpPathBookmark>>(&bytes) {
        Ok(bookmarks) => bookmarks,
        Err(error) => {
            eprintln!(
                "failed to parse SFTP path bookmarks {}: {error}",
                path.display()
            );
            Vec::new()
        }
    }
}

pub(in crate::workspace::sftp) fn save_sftp_path_bookmarks(
    settings_path: &std::path::Path,
    bookmarks: &[SftpPathBookmark],
) -> std::io::Result<()> {
    let bytes = serde_json::to_vec_pretty(bookmarks)
        .map_err(|error| std::io::Error::other(error.to_string()))?;
    oxideterm_atomic_file::durable_write(&sftp_path_bookmarks_path(settings_path), &bytes)
}

pub(in crate::workspace::sftp) fn new_sftp_path_bookmark_id() -> String {
    // Mirrors the file manager bookmark id shape; ids only need to be unique
    // within this file and are never used as persistence keys.
    let millis = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or_default();
    format!("bookmark-{millis}")
}

/// Pick the smallest unused `{{index}}` substitution within the same endpoint.
pub(in crate::workspace::sftp) fn default_sftp_bookmark_name(
    bookmarks: &[SftpPathBookmark],
    remote_key: &str,
    template: &str,
) -> String {
    // A malformed catalog must not loop forever on an unsubstitutable template.
    if !template.contains("{{index}}") {
        return template.to_string();
    }
    let mut index = 1usize;
    loop {
        let candidate = template.replace("{{index}}", &index.to_string());
        let taken = bookmarks
            .iter()
            .any(|bookmark| bookmark.remote_key == remote_key && bookmark.name == candidate);
        if !taken {
            return candidate;
        }
        index += 1;
    }
}

/// Build the durable key used to filter bookmarks for a node-backed endpoint.
/// Saved connections keep their identity across restarts; direct connections
/// fall back to the visible endpoint so the bookmark survives reconnects.
pub(in crate::workspace::sftp) fn sftp_bookmark_node_remote_key(
    saved_connection_id: Option<&str>,
    username: &str,
    host: &str,
    port: u16,
) -> String {
    match saved_connection_id
        .map(str::trim)
        .filter(|id| !id.is_empty())
    {
        Some(id) => format!("saved:{id}"),
        None => format!("endpoint:{username}@{host}:{port}"),
    }
}

impl WorkspaceApp {
    pub(in crate::workspace::sftp) fn ensure_sftp_path_bookmarks_loaded(&mut self) {
        if self.sftp_path_bookmarks_loaded {
            return;
        }
        self.sftp_path_bookmarks = load_sftp_path_bookmarks(self.settings_store.path());
        self.sftp_path_bookmarks_loaded = true;
    }

    /// Returns whether the durable write succeeded; callers must not report a
    /// success that exists only in memory.
    pub(in crate::workspace::sftp) fn persist_sftp_path_bookmarks(&self, cx: &App) -> bool {
        // An unloaded list is indistinguishable from an empty one; never let a
        // first render overwrite the persisted file.
        if !self.sftp_path_bookmarks_loaded {
            return false;
        }
        if let Err(error) =
            save_sftp_path_bookmarks(self.settings_store.path(), &self.sftp_path_bookmarks)
        {
            self.push_sftp_toast(
                self.i18n.t("sftp.bookmarks.save_failed"),
                Some(error.to_string()),
                TerminalNoticeVariant::Error,
                cx,
            );
            return false;
        }
        true
    }

    pub(in crate::workspace::sftp) fn sftp_bookmark_key_for_remote(
        &self,
        remote_id: &SftpRemoteId,
        _cx: &App,
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

#[cfg(test)]
mod tests {
    use super::*;

    fn bookmark(id: &str, name: &str, remote_key: &str) -> SftpPathBookmark {
        SftpPathBookmark {
            id: id.to_string(),
            name: name.to_string(),
            local_path: format!("/local/{id}"),
            remote_path: format!("/remote/{id}"),
            remote_key: remote_key.to_string(),
            created_at: 1,
        }
    }

    #[test]
    fn default_name_uses_smallest_free_index_per_endpoint() {
        let template = "书签 {{index}}";
        assert_eq!(
            default_sftp_bookmark_name(&[], "saved:a", template),
            "书签 1"
        );

        let bookmarks = vec![
            bookmark("1", "书签 1", "saved:a"),
            bookmark("2", "书签 2", "saved:a"),
        ];
        assert_eq!(
            default_sftp_bookmark_name(&bookmarks, "saved:a", template),
            "书签 3"
        );

        let bookmarks = vec![
            bookmark("1", "书签 1", "saved:a"),
            bookmark("3", "书签 3", "saved:a"),
        ];
        assert_eq!(
            default_sftp_bookmark_name(&bookmarks, "saved:a", template),
            "书签 2"
        );
        // Another endpoint may reuse the same display name.
        assert_eq!(
            default_sftp_bookmark_name(&bookmarks, "saved:b", template),
            "书签 1"
        );
    }

    #[test]
    fn node_remote_key_prefers_saved_connection_identity() {
        assert_eq!(
            sftp_bookmark_node_remote_key(Some("conn-7"), "root", "example.com", 22),
            "saved:conn-7"
        );
        assert_eq!(
            sftp_bookmark_node_remote_key(None, "root", "example.com", 22),
            "endpoint:root@example.com:22"
        );
        assert_ne!(
            sftp_bookmark_node_remote_key(None, "root", "example.com", 22),
            sftp_bookmark_node_remote_key(None, "root", "example.com", 2222)
        );
    }

    #[test]
    fn bookmarks_roundtrip_with_expected_json_shape() {
        let directory = std::env::temp_dir().join(format!(
            "oxideterm-sftp-bookmarks-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|duration| duration.as_nanos())
                .unwrap_or_default()
        ));
        std::fs::create_dir_all(&directory).expect("create temp directory");
        let settings_path = directory.join("settings.json");
        let bookmarks = vec![
            bookmark("one", "Work", "saved:a"),
            bookmark("two", "Logs", "saved:a"),
        ];
        let mut expected = bookmarks.clone();
        expected[0].created_at = 10;
        expected[1].created_at = 20;

        save_sftp_path_bookmarks(&settings_path, &expected).expect("save bookmarks");
        let bytes = std::fs::read(sftp_path_bookmarks_path(&settings_path)).expect("read file");
        let text = String::from_utf8(bytes).expect("utf8");
        assert!(text.contains("\"localPath\""));
        assert!(text.contains("\"remotePath\""));
        assert!(text.contains("\"remoteKey\""));
        assert!(text.contains("\"createdAt\""));
        assert!(text.starts_with("[\n  {\n    \"id\": \"one\""));

        let loaded = load_sftp_path_bookmarks(&settings_path);
        assert_eq!(loaded, expected);
        std::fs::remove_dir_all(&directory).expect("remove temp directory");
    }
}
