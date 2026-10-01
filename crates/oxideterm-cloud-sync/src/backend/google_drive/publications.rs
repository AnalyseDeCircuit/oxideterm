// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use super::*;

pub(super) fn google_drive_scoped_object_name(config: &CloudSyncSettings, path: &str) -> String {
    if let Some(relative) = path.strip_prefix("sync-v3/") {
        format!(
            "sync-v3__{}__{}",
            sha256_hex(config.namespace.as_bytes()).replace(':', "-"),
            relative.replace('/', "__")
        )
    } else {
        google_drive_object_name(path)
    }
}

impl CloudSyncBackend {
    pub(in crate::backend) async fn list_google_drive_publications(
        &self,
        config: &CloudSyncSettings,
        secrets: &CloudSyncSecrets,
    ) -> Result<Vec<String>> {
        let prefix = google_drive_scoped_object_name(config, "sync-v3/");
        let mut token = None::<String>;
        let mut seen = BTreeSet::new();
        let mut paths = Vec::new();
        loop {
            let query = format!(
                "name contains {} and trashed = false",
                google_drive_query_literal(&prefix)
            );
            let mut request = self
                .client
                .get(format!("{GOOGLE_DRIVE_API_BASE}/files"))
                .headers(google_drive_headers(secrets)?)
                .query(&[
                    ("spaces", "appDataFolder"),
                    ("fields", "nextPageToken,files(name)"),
                    ("pageSize", "1000"),
                    ("q", &query),
                ]);
            if let Some(token) = &token {
                request = request.query(&[("pageToken", token.as_str())]);
            }
            let response = execute_cloud_request(request).await?;
            if !response.status().is_success() {
                bail!(
                    "Google Drive cannot enumerate sync snapshots ({})",
                    response.status().as_u16()
                );
            }
            let value = response.json::<Value>().await?;
            for file in value
                .get("files")
                .and_then(Value::as_array)
                .context("Google Drive omitted sync files")?
            {
                if let Some(path) = file
                    .get("name")
                    .and_then(Value::as_str)
                    .and_then(|name| name.strip_prefix(&prefix))
                {
                    paths.push(format!("sync-v3/{}", path.replacen("__", "/", 1)));
                }
            }
            let Some(next) = value
                .get("nextPageToken")
                .and_then(Value::as_str)
                .filter(|value| !value.is_empty())
            else {
                break;
            };
            if !seen.insert(next.to_string()) {
                bail!("Google Drive repeated its page token");
            }
            token = Some(next.to_string());
        }
        Ok(paths)
    }

    pub(in crate::backend) async fn delete_google_drive_publication(
        &self,
        config: &CloudSyncSettings,
        secrets: &CloudSyncSecrets,
        path: &str,
    ) -> Result<()> {
        let name = google_drive_scoped_object_name(config, path);
        if let Some(file) = self.find_google_drive_file(secrets, &name).await? {
            crate::backend::publications::check_delete(
                execute_cloud_request(
                    self.client
                        .delete(format!(
                            "{GOOGLE_DRIVE_API_BASE}/files/{}",
                            encode_component(&file.id)
                        ))
                        .headers(google_drive_headers(secrets)?),
                )
                .await?,
            )?;
        }
        Ok(())
    }
}
