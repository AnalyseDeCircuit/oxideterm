// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use super::*;

impl CloudSyncBackend {
    pub(in crate::backend) async fn list_http_json_publications(
        &self,
        config: &CloudSyncSettings,
        secrets: &CloudSyncSecrets,
    ) -> Result<Vec<String>> {
        let endpoint = join_url(
            &config.endpoint,
            &format!(
                "v1/namespaces/{}/objects",
                encode_component(&config.namespace)
            ),
        );
        let mut cursor = None::<String>;
        let mut seen = BTreeSet::new();
        let mut files = Vec::new();
        loop {
            let mut request = self
                .client
                .get(&endpoint)
                .headers(self.http_json_auth_headers(config, secrets)?)
                .query(&[("prefix", "sync-v3/")]);
            if let Some(cursor) = &cursor {
                request = request.query(&[("cursor", cursor.as_str())]);
            }
            let response = execute_cloud_request(request).await?;
            if matches!(response.status().as_u16(), 404 | 405 | 501) {
                bail!(
                    "sync_protocol_upgrade_required: HTTP JSON server must support v3 object enumeration"
                );
            }
            if !response.status().is_success() {
                bail!(
                    "HTTP JSON cannot enumerate sync snapshots ({})",
                    response.status().as_u16()
                );
            }
            let value = response.json::<Value>().await?;
            let entries = value
                .get("objects")
                .and_then(Value::as_array)
                .context("HTTP JSON omitted sync objects")?;
            for entry in entries {
                let path = entry
                    .get("path")
                    .and_then(Value::as_str)
                    .context("HTTP JSON omitted object path")?;
                if !path.starts_with("sync-v3/") {
                    bail!("HTTP JSON returned an unrelated object");
                }
                files.push(path.to_string());
            }
            let Some(next) = value
                .get("nextCursor")
                .and_then(Value::as_str)
                .filter(|value| !value.is_empty())
            else {
                break;
            };
            if !seen.insert(next.to_string()) {
                bail!("HTTP JSON repeated its sync cursor");
            }
            cursor = Some(next.to_string());
        }
        Ok(files)
    }

    pub(in crate::backend) async fn delete_http_json_publication(
        &self,
        config: &CloudSyncSettings,
        secrets: &CloudSyncSecrets,
        path: &str,
    ) -> Result<()> {
        crate::backend::publications::check_delete(
            execute_cloud_request(
                self.client
                    .delete(join_url(
                        &config.endpoint,
                        &format!(
                            "v1/namespaces/{}/objects/{}",
                            encode_component(&config.namespace),
                            encode_path_segments(path)
                        ),
                    ))
                    .headers(self.http_json_auth_headers(config, secrets)?),
            )
            .await?,
        )
    }
}
