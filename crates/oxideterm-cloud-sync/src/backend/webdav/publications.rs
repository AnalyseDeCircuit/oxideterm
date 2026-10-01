// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use super::*;

impl CloudSyncBackend {
    pub(in crate::backend) async fn list_webdav_publications(
        &self,
        config: &CloudSyncSettings,
        secrets: &CloudSyncSecrets,
    ) -> Result<Vec<String>> {
        let root = Url::parse(&format!("{}/", webdav_object_url(config, "sync-v3")))?;
        let mut directories = vec![root.clone()];
        let mut files = Vec::new();
        while let Some(directory) = directories.pop() {
            let response = execute_cloud_request(
                self.client
                    .request(Method::from_bytes(b"PROPFIND")?, directory.clone())
                    .headers(self.webdav_auth_headers(config, secrets)?)
                    .header("Depth", "1"),
            )
            .await?;
            if response.status() == StatusCode::NOT_FOUND {
                continue;
            }
            if !response.status().is_success() {
                bail!(
                    "WebDAV cannot enumerate sync snapshots ({})",
                    response.status().as_u16()
                );
            }
            let body = response.bytes().await?;
            for href in crate::backend::publications::xml_values(&body, b"href")? {
                let url = directory.join(&href)?;
                if url.origin() != root.origin() {
                    bail!("WebDAV returned an unrelated sync location");
                }
                let root_path = percent_encoding::percent_decode_str(root.path()).decode_utf8()?;
                let path = percent_encoding::percent_decode_str(url.path()).decode_utf8()?;
                let Some(relative) = path.strip_prefix(root_path.as_ref()) else {
                    continue;
                };
                let relative = relative.trim_matches('/');
                if relative.is_empty() {
                    continue;
                }
                if !relative.contains('/') {
                    if directory == root && uuid::Uuid::parse_str(relative).is_ok() {
                        directories.push(Url::parse(&format!(
                            "{}/",
                            webdav_object_url(config, &format!("sync-v3/{relative}"))
                        ))?);
                    }
                } else if relative.ends_with(".oxide") {
                    files.push(format!("sync-v3/{relative}"));
                }
            }
        }
        Ok(files)
    }

    pub(in crate::backend) async fn delete_webdav_publication(
        &self,
        config: &CloudSyncSettings,
        secrets: &CloudSyncSecrets,
        path: &str,
    ) -> Result<()> {
        crate::backend::publications::check_delete(
            execute_cloud_request(
                self.client
                    .delete(webdav_object_url(config, path))
                    .headers(self.webdav_auth_headers(config, secrets)?),
            )
            .await?,
        )
    }
}
