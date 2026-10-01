// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use super::*;

pub(super) fn onedrive_sync_path(config: &CloudSyncSettings, path: &str) -> String {
    if path == "sync-v3" || path.starts_with("sync-v3/") {
        format!(
            "namespaces/{}/{path}",
            sha256_hex(config.namespace.as_bytes()).replace(':', "-")
        )
    } else {
        path.to_string()
    }
}

impl CloudSyncBackend {
    pub(in crate::backend) async fn list_onedrive_publications(
        &self,
        config: &CloudSyncSettings,
        secrets: &CloudSyncSecrets,
    ) -> Result<Vec<String>> {
        let mut folders = vec!["sync-v3".to_string()];
        let mut files = Vec::new();
        while let Some(folder) = folders.pop() {
            let mut next = Some(onedrive_children_url(
                config,
                &onedrive_sync_path(config, &folder),
            ));
            let mut seen = BTreeSet::new();
            while let Some(url) = next.take() {
                let parsed = Url::parse(&url)?;
                if parsed.origin() != Url::parse(MICROSOFT_GRAPH_BASE)?.origin() {
                    bail!("OneDrive returned an unrelated page URL");
                }
                if !seen.insert(url.clone()) {
                    bail!("OneDrive repeated its page URL");
                }
                let response = execute_cloud_request(
                    self.client.get(&url).headers(onedrive_headers(secrets)?),
                )
                .await?;
                if response.status() == StatusCode::NOT_FOUND {
                    break;
                }
                if !response.status().is_success() {
                    bail!(
                        "OneDrive cannot enumerate sync snapshots ({})",
                        response.status().as_u16()
                    );
                }
                let value = response.json::<Value>().await?;
                for entry in value
                    .get("value")
                    .and_then(Value::as_array)
                    .context("OneDrive omitted sync files")?
                {
                    let name = entry
                        .get("name")
                        .and_then(Value::as_str)
                        .context("OneDrive omitted sync filename")?;
                    if name.contains('/') {
                        bail!("Invalid OneDrive sync filename");
                    }
                    if entry.get("folder").is_some()
                        && folder == "sync-v3"
                        && uuid::Uuid::parse_str(name).is_ok()
                    {
                        folders.push(format!("{folder}/{name}"));
                    } else if entry.get("file").is_some() && name.ends_with(".oxide") {
                        files.push(format!("{folder}/{name}"));
                    }
                }
                next = value
                    .get("@odata.nextLink")
                    .and_then(Value::as_str)
                    .map(str::to_owned);
            }
        }
        Ok(files)
    }

    pub(in crate::backend) async fn delete_onedrive_publication(
        &self,
        config: &CloudSyncSettings,
        secrets: &CloudSyncSecrets,
        path: &str,
    ) -> Result<()> {
        crate::backend::publications::check_delete(
            execute_cloud_request(
                self.client
                    .delete(onedrive_item_url(config, &onedrive_sync_path(config, path)))
                    .headers(onedrive_headers(secrets)?),
            )
            .await?,
        )
    }
}
