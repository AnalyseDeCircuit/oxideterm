// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use super::*;

impl CloudSyncBackend {
    pub(in crate::backend) async fn list_dropbox_publications(
        &self,
        config: &CloudSyncSettings,
        secrets: &CloudSyncSecrets,
    ) -> Result<Vec<String>> {
        let root = dropbox_object_path(config, "sync-v3");
        let mut cursor = None::<String>;
        let mut seen = BTreeSet::new();
        let mut files = Vec::new();
        loop {
            let (endpoint, body) = if let Some(cursor) = &cursor {
                ("files/list_folder/continue", json!({"cursor": cursor}))
            } else {
                (
                    "files/list_folder",
                    json!({"path": root, "recursive": true}),
                )
            };
            let response = execute_cloud_request(
                self.client
                    .post(format!("{DROPBOX_API_BASE}/{endpoint}"))
                    .headers(dropbox_headers(secrets)?)
                    .header(CONTENT_TYPE, "application/json")
                    .body(serde_json::to_vec(&body)?),
            )
            .await?;
            let status = response.status();
            let value = response.json::<Value>().await?;
            if status == StatusCode::CONFLICT
                && value
                    .get("error_summary")
                    .and_then(Value::as_str)
                    .is_some_and(|error| error.starts_with("path/not_found"))
            {
                return Ok(Vec::new());
            }
            if !status.is_success() {
                bail!(
                    "Dropbox cannot enumerate sync snapshots ({})",
                    status.as_u16()
                );
            }
            let entries = value
                .get("entries")
                .and_then(Value::as_array)
                .context("Dropbox omitted sync entries")?;
            for entry in entries {
                if entry.get(".tag").and_then(Value::as_str) != Some("file") {
                    continue;
                }
                if let Some(path) = entry
                    .get("path_display")
                    .and_then(Value::as_str)
                    .and_then(|path| path.strip_prefix(&format!("{root}/")))
                {
                    files.push(format!("sync-v3/{path}"));
                }
            }
            if !value
                .get("has_more")
                .and_then(Value::as_bool)
                .unwrap_or(false)
            {
                break;
            }
            let next = value
                .get("cursor")
                .and_then(Value::as_str)
                .context("Dropbox omitted sync cursor")?
                .to_string();
            if !seen.insert(next.clone()) {
                bail!("Dropbox repeated its sync cursor");
            }
            cursor = Some(next);
        }
        Ok(files)
    }

    pub(in crate::backend) async fn delete_dropbox_publication(
        &self,
        config: &CloudSyncSettings,
        secrets: &CloudSyncSecrets,
        path: &str,
    ) -> Result<()> {
        let response = execute_cloud_request(
            self.client
                .post(format!("{DROPBOX_API_BASE}/files/delete_v2"))
                .headers(dropbox_headers(secrets)?)
                .header(CONTENT_TYPE, "application/json")
                .body(serde_json::to_vec(
                    &json!({"path": dropbox_object_path(config, path)}),
                )?),
        )
        .await?;
        if response.status() == StatusCode::CONFLICT {
            let value = response.json::<Value>().await?;
            if value
                .get("error_summary")
                .and_then(Value::as_str)
                .is_some_and(|error| error.starts_with("path_lookup/not_found"))
            {
                return Ok(());
            }
            bail!("Dropbox cannot delete sync snapshot");
        }
        crate::backend::publications::check_delete(response)
    }
}
