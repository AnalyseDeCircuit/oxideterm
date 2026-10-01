// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use super::*;
use crate::sync_v3::{PendingPublication, PublicationId};

impl CloudSyncBackend {
    pub async fn list_publications(
        &self,
        config: &CloudSyncSettings,
        secrets: &CloudSyncSecrets,
    ) -> Result<Vec<PublicationId>> {
        validate_namespace(config)?;
        let paths = match config.backend_type {
            BackendType::Webdav => self.list_webdav_publications(config, secrets).await?,
            BackendType::S3 => self.list_s3_publications(config, secrets).await?,
            BackendType::Dropbox => self.list_dropbox_publications(config, secrets).await?,
            BackendType::OneDrive => self.list_onedrive_publications(config, secrets).await?,
            BackendType::GoogleDrive => {
                self.list_google_drive_publications(config, secrets).await?
            }
            BackendType::GithubGist => self.list_gist_publications(config, secrets).await?,
            BackendType::Git => self.list_git_publications(config, secrets).await?,
            BackendType::HttpJson => self.list_http_json_publications(config, secrets).await?,
        };
        let mut result = BTreeSet::new();
        for path in paths {
            if path.ends_with(".oxide") {
                result.insert(PublicationId::parse(&path)?);
            }
        }
        Ok(result.into_iter().collect())
    }

    /// An acknowledgement is based on the stored bytes, including after a lost
    /// HTTP response. The same operation never generates new ciphertext on retry.
    pub async fn publish_replica(
        &self,
        config: &CloudSyncSettings,
        secrets: &CloudSyncSecrets,
        pending: &PendingPublication,
    ) -> Result<()> {
        let id = PublicationId::parse(&pending.path)?;
        if sha256_hex(&pending.bytes) != format!("sha256:{}", id.digest) {
            bail!("Invalid pending cloud sync publication");
        }
        if let Some(existing) = self
            .read_remote_object(config, secrets, &pending.path)
            .await?
        {
            if existing.bytes != pending.bytes {
                bail!("Cloud sync publication already exists with different contents");
            }
            return Ok(());
        }
        let written = self
            .write_remote_object(
                config,
                secrets,
                &pending.path,
                pending.bytes.clone(),
                Some(OXIDE_CONTENT_TYPE),
            )
            .await;
        match self
            .read_remote_object(config, secrets, &pending.path)
            .await
        {
            Ok(Some(object)) if object.bytes == pending.bytes => Ok(()),
            _ => {
                written?;
                bail!("Cloud sync publication could not be verified; upload remains pending");
            }
        }
    }

    pub async fn delete_publication(
        &self,
        config: &CloudSyncSettings,
        secrets: &CloudSyncSecrets,
        id: &PublicationId,
    ) -> Result<()> {
        let path = id.path();
        PublicationId::parse(&path)?;
        match config.backend_type {
            BackendType::Webdav => self.delete_webdav_publication(config, secrets, &path).await,
            BackendType::S3 => self.delete_s3_publication(config, secrets, &path).await,
            BackendType::Dropbox => {
                self.delete_dropbox_publication(config, secrets, &path)
                    .await
            }
            BackendType::OneDrive => {
                self.delete_onedrive_publication(config, secrets, &path)
                    .await
            }
            BackendType::GoogleDrive => {
                self.delete_google_drive_publication(config, secrets, &path)
                    .await
            }
            BackendType::GithubGist => self.delete_gist_publication(config, secrets, &path).await,
            BackendType::Git => self.delete_git_publication(config, secrets, &path).await,
            BackendType::HttpJson => {
                self.delete_http_json_publication(config, secrets, &path)
                    .await
            }
        }
    }
}

pub(super) fn xml_values(bytes: &[u8], name: &[u8]) -> Result<Vec<String>> {
    let mut reader = quick_xml::Reader::from_reader(bytes);
    let mut result = Vec::new();
    loop {
        match reader
            .read_event()
            .map_err(|_| anyhow::anyhow!("Invalid cloud storage XML response"))?
        {
            quick_xml::events::Event::Start(tag) if tag.local_name().as_ref() == name => {
                let text = reader
                    .read_text(tag.name())
                    .map_err(|_| anyhow::anyhow!("Invalid cloud storage XML value"))?;
                result.push(
                    quick_xml::escape::unescape(&text)
                        .map_err(|_| anyhow::anyhow!("Invalid cloud storage XML encoding"))?
                        .into_owned(),
                );
            }
            quick_xml::events::Event::Eof => break,
            _ => {}
        }
    }
    Ok(result)
}

pub(super) fn check_delete(response: HttpResponseSnapshot) -> Result<()> {
    if response.status().is_success() || response.status() == StatusCode::NOT_FOUND {
        Ok(())
    } else {
        bail!(
            "Cloud sync object deletion failed ({})",
            response.status().as_u16()
        );
    }
}
