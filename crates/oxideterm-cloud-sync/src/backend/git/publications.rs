// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use super::*;

impl CloudSyncBackend {
    async fn sync_git_tree(
        &self,
        config: &CloudSyncSettings,
        secrets: &CloudSyncSecrets,
        revision: &str,
    ) -> Result<Option<Vec<Value>>> {
        let (owner, repo) = parse_git_repository(config)?;
        let endpoint = if config.endpoint.trim().is_empty() {
            DEFAULT_GIT_API_ENDPOINT.to_string()
        } else {
            trim_trailing_slash(&config.endpoint)
        };
        let response = execute_cloud_request(
            self.client
                .get(format!(
                    "{endpoint}/repos/{}/{}/git/trees/{}",
                    encode_component(&owner),
                    encode_component(&repo),
                    encode_component(revision)
                ))
                .headers(git_headers(secrets, "application/vnd.github+json")?),
        )
        .await?;
        if response.status() == StatusCode::NOT_FOUND {
            return Ok(None);
        }
        if !response.status().is_success() {
            bail!(
                "Git cannot enumerate sync snapshots ({})",
                response.status().as_u16()
            );
        }
        let mut value = response.json::<Value>().await?;
        if value.get("truncated").and_then(Value::as_bool) == Some(true) {
            bail!("Git returned an incomplete sync tree");
        }
        match value.get_mut("tree").map(Value::take) {
            Some(Value::Array(entries)) => Ok(Some(entries)),
            _ => bail!("Git omitted sync tree"),
        }
    }

    pub(in crate::backend) async fn list_git_publications(
        &self,
        config: &CloudSyncSettings,
        secrets: &CloudSyncSecrets,
    ) -> Result<Vec<String>> {
        let Some(mut entries) = self
            .sync_git_tree(config, secrets, &git_branch(config))
            .await?
        else {
            return Ok(Vec::new());
        };
        for component in git_object_path(config, "sync-v3").split('/') {
            let Some(sha) = entries
                .iter()
                .find(|entry| {
                    entry.get("path").and_then(Value::as_str) == Some(component)
                        && entry.get("type").and_then(Value::as_str) == Some("tree")
                })
                .and_then(|entry| entry.get("sha"))
                .and_then(Value::as_str)
            else {
                return Ok(Vec::new());
            };
            entries = self
                .sync_git_tree(config, secrets, sha)
                .await?
                .context("Git sync tree disappeared")?;
        }
        let mut files = Vec::new();
        for entry in entries {
            let Some(writer) = entry
                .get("path")
                .and_then(Value::as_str)
                .filter(|name| uuid::Uuid::parse_str(name).is_ok())
            else {
                continue;
            };
            if entry.get("type").and_then(Value::as_str) != Some("tree") {
                continue;
            }
            let sha = entry
                .get("sha")
                .and_then(Value::as_str)
                .context("Git omitted sync tree identity")?;
            for object in self
                .sync_git_tree(config, secrets, sha)
                .await?
                .context("Git sync tree disappeared")?
            {
                if object.get("type").and_then(Value::as_str) == Some("blob") {
                    let name = object
                        .get("path")
                        .and_then(Value::as_str)
                        .context("Git omitted sync filename")?;
                    if name.ends_with(".oxide") {
                        files.push(format!("sync-v3/{writer}/{name}"));
                    }
                }
            }
        }
        Ok(files)
    }

    pub(in crate::backend) async fn delete_git_publication(
        &self,
        config: &CloudSyncSettings,
        secrets: &CloudSyncSecrets,
        path: &str,
    ) -> Result<()> {
        let path = git_object_path(config, path);
        if let Some(file) = self.fetch_git_file(config, secrets, &path).await? {
            let sha = file.sha.context("Git omitted sync object identity")?;
            crate::backend::publications::check_delete(execute_cloud_request(self.client.delete(git_contents_url(config, &path, false)?)
                .headers(git_headers(secrets, "application/vnd.github+json")?).header(CONTENT_TYPE, "application/json")
                .body(serde_json::to_vec(&json!({"message": "Remove superseded OxideTerm sync snapshot", "sha": sha, "branch": git_branch(config)}))?)).await?)?;
        }
        Ok(())
    }
}
