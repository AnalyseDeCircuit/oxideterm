// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use super::*;

impl CloudSyncBackend {
    pub(in crate::backend) async fn list_s3_publications(
        &self,
        config: &CloudSyncSettings,
        secrets: &CloudSyncSecrets,
    ) -> Result<Vec<String>> {
        let namespace = trim_slashes(&config.namespace);
        let prefix = if namespace.is_empty() {
            "sync-v3/".into()
        } else {
            format!("{namespace}/sync-v3/")
        };
        let mut token = None::<String>;
        let mut seen = std::collections::BTreeSet::new();
        let mut files = Vec::new();
        loop {
            let mut url = join_s3_object_url(&config.endpoint, &config.s3_bucket, "")?;
            url.query_pairs_mut()
                .append_pair("list-type", "2")
                .append_pair("prefix", &prefix);
            if let Some(token) = &token {
                url.query_pairs_mut()
                    .append_pair("continuation-token", token);
            }
            let response = self
                .s3_request(Method::GET, &url, config, secrets, None, HeaderMap::new())
                .await?;
            if !response.status().is_success() {
                bail!(
                    "S3 cannot enumerate sync snapshots ({})",
                    response.status().as_u16()
                );
            }
            let body = response.bytes().await?;
            for key in crate::backend::publications::xml_values(&body, b"Key")? {
                if let Some(relative) = key.strip_prefix(&prefix) {
                    files.push(format!("sync-v3/{relative}"));
                }
            }
            let truncated = crate::backend::publications::xml_values(&body, b"IsTruncated")?
                .first()
                .is_some_and(|value| value == "true");
            if !truncated {
                break;
            }
            let next = crate::backend::publications::xml_values(&body, b"NextContinuationToken")?
                .into_iter()
                .next()
                .ok_or_else(|| anyhow::anyhow!("S3 omitted its continuation token"))?;
            if !seen.insert(next.clone()) {
                bail!("S3 repeated its continuation token");
            }
            token = Some(next);
        }
        Ok(files)
    }

    pub(in crate::backend) async fn delete_s3_publication(
        &self,
        config: &CloudSyncSettings,
        secrets: &CloudSyncSecrets,
        path: &str,
    ) -> Result<()> {
        crate::backend::publications::check_delete(
            self.s3_request(
                Method::DELETE,
                &s3_object_url(config, path)?,
                config,
                secrets,
                None,
                HeaderMap::new(),
            )
            .await?,
        )
    }
}
