// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

//! Quick-start guide copy model for Cloud Sync backends.

use oxideterm_cloud_sync::BackendType;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CloudSyncGuideSpec {
    pub title_key: &'static str,
    pub description_key: &'static str,
    pub examples: Vec<CloudSyncGuideExampleSpec>,
    pub warning_key: Option<&'static str>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CloudSyncGuideExampleSpec {
    pub label_key: &'static str,
    pub value_key: &'static str,
}

pub const CLOUD_SYNC_GUIDE_STEP_KEYS: &[&str] = &[
    "plugin.cloud_sync.guide.step_choose_backend",
    "plugin.cloud_sync.guide.step_fill_fields",
    "plugin.cloud_sync.guide.step_save",
    "plugin.cloud_sync.guide.step_check",
    "plugin.cloud_sync.guide.step_upload",
    "plugin.cloud_sync.guide.step_pull",
];

pub fn cloud_sync_guide_spec(backend_type: &BackendType) -> CloudSyncGuideSpec {
    match backend_type {
        BackendType::Webdav => CloudSyncGuideSpec {
            title_key: "plugin.cloud_sync.guide.webdav_title",
            description_key: "plugin.cloud_sync.guide.webdav_description",
            examples: vec![
                CloudSyncGuideExampleSpec {
                    label_key: "plugin.cloud_sync.settings.endpoint",
                    value_key: "plugin.cloud_sync.guide.webdav_example_endpoint",
                },
                CloudSyncGuideExampleSpec {
                    label_key: "plugin.cloud_sync.settings.namespace",
                    value_key: "plugin.cloud_sync.guide.webdav_example_namespace",
                },
                CloudSyncGuideExampleSpec {
                    label_key: "plugin.cloud_sync.settings.basic_username",
                    value_key: "plugin.cloud_sync.guide.webdav_example_username",
                },
                CloudSyncGuideExampleSpec {
                    label_key: "plugin.cloud_sync.settings.basic_password",
                    value_key: "plugin.cloud_sync.guide.webdav_example_password",
                },
            ],
            warning_key: Some("plugin.cloud_sync.guide.webdav_duplicate_warning"),
        },
        BackendType::HttpJson => CloudSyncGuideSpec {
            title_key: "plugin.cloud_sync.guide.http_json_title",
            description_key: "plugin.cloud_sync.guide.http_json_description",
            examples: vec![
                CloudSyncGuideExampleSpec {
                    label_key: "plugin.cloud_sync.settings.endpoint",
                    value_key: "plugin.cloud_sync.guide.http_json_example_endpoint",
                },
                CloudSyncGuideExampleSpec {
                    label_key: "plugin.cloud_sync.settings.namespace",
                    value_key: "plugin.cloud_sync.guide.http_json_example_namespace",
                },
            ],
            warning_key: None,
        },
        BackendType::Dropbox => backend_notes_spec(
            "plugin.cloud_sync.backend.dropbox",
            "plugin.cloud_sync.notes.backend_dropbox",
        ),
        BackendType::GithubGist => backend_notes_spec(
            "plugin.cloud_sync.backend.github_gist",
            "plugin.cloud_sync.notes.backend_github_gist",
        ),
        BackendType::Git => backend_notes_spec(
            "plugin.cloud_sync.backend.git",
            "plugin.cloud_sync.notes.backend_git",
        ),
        BackendType::S3 => backend_notes_spec(
            "plugin.cloud_sync.backend.s3",
            "plugin.cloud_sync.notes.backend_s3",
        ),
    }
}

fn backend_notes_spec(
    title_key: &'static str,
    description_key: &'static str,
) -> CloudSyncGuideSpec {
    CloudSyncGuideSpec {
        title_key,
        description_key,
        examples: Vec::new(),
        warning_key: None,
    }
}
