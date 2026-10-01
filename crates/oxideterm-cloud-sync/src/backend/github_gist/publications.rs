// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use super::*;

impl CloudSyncBackend {
    pub(in crate::backend) async fn list_gist_publications(
        &self,
        config: &CloudSyncSettings,
        secrets: &CloudSyncSecrets,
    ) -> Result<Vec<String>> {
        let Some(gist) = self.fetch_gist_value(config, secrets).await? else {
            return Ok(Vec::new());
        };
        if gist.get("truncated").and_then(Value::as_bool) == Some(true) {
            bail!("GitHub returned an incomplete sync file listing");
        }
        let prefix = gist_v3_prefix(config);
        let files = gist
            .get("files")
            .and_then(Value::as_object)
            .context("GitHub omitted sync files")?;
        Ok(files
            .keys()
            .filter_map(|filename| filename.strip_prefix(&prefix))
            .map(|path| format!("sync-v3/{}", path.replacen("--", "/", 1)))
            .collect())
    }

    pub(in crate::backend) async fn delete_gist_publication(
        &self,
        config: &CloudSyncSettings,
        secrets: &CloudSyncSecrets,
        path: &str,
    ) -> Result<()> {
        crate::backend::publications::check_delete(
            execute_cloud_request(
                self.client
                    .patch(gist_url(config)?)
                    .headers(gist_headers(secrets)?)
                    .header(CONTENT_TYPE, "application/json")
                    .body(serde_json::to_vec(
                        &json!({"files": {gist_object_filename(config, path): Value::Null}}),
                    )?),
            )
            .await?,
        )
    }
}
