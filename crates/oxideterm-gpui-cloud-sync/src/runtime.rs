use crate::entity::{
    CloudSyncLocalSnapshotCache, CloudSyncUiIntent, CloudSyncUploadDiffCache,
    CloudSyncWorkspaceEntity, CloudSyncWorkspaceEvent,
};
use crate::*;
use chrono::Utc;
use gpui::{App, Entity};
use oxideterm_cloud_sync::{
    AuthMode, CloudSyncStatus, RawSyncScope, normalize_sync_scope,
    operation::{
        ApplyStructuredPreviewOutcome, LegacyPreview, PreparedSync, SyncDirection, SyncOutcome,
        UploadOutcome,
    },
    secret_keys,
    secrets::{CloudSyncKeychainSecretProvider, backend_uses_auth_mode},
    service::{CloudSyncLocalSnapshot, build_local_snapshot},
    state::{CloudSyncHistorySummary, CloudSyncPersistedState},
};
use oxideterm_connections::{
    ConnectionStore,
    oxide_file::{EncryptedPortableSecret, ImportResultEnvelope},
};
use oxideterm_forwarding::ForwardingRegistry;
use oxideterm_i18n::I18n;
use oxideterm_settings::{PersistedSettings, SettingsStore};
use oxideterm_settings_model::CloudSyncFormDraft;
use oxideterm_theme::ThemeTokens;
use std::{future::Future, path::PathBuf, pin::Pin};

mod causal;
mod configuration;
mod local_file;
mod maintenance;
mod operations;
mod snapshot;
pub(crate) use local_file::LocalFileResult;

pub use operations::format_cloud_sync_error;
pub use snapshot::{CloudSyncDiffRenderData, CloudSyncSources};

#[derive(Clone, Copy)]
pub enum CloudSyncNoticeKind {
    Default,
    Success,
    Error,
}

pub enum CloudSyncHostEffect {
    Notice {
        title: String,
        description: Option<String>,
        kind: CloudSyncNoticeKind,
    },
    RefreshOwners(Box<PersistedSettings>),
    ApplyPreview(Box<CloudSyncApplyUiOutcome>),
    AnimateTab(CloudSyncTab),
    OpenUrl(String),
}

pub type CloudSyncFileDialog =
    fn(&mut App, bool, &I18n) -> Pin<Box<dyn Future<Output = Result<Option<PathBuf>, String>>>>;

/// Borrows the authoritative application stores for one synchronous controller
/// transaction. Window effects are returned only after those borrows end.
pub struct CloudSyncRuntime<'a> {
    pub cloud_sync: Entity<CloudSyncWorkspaceEntity>,
    pub connection_store: &'a mut ConnectionStore,
    pub settings_store: &'a mut SettingsStore,
    pub forwarding_registry: &'a ForwardingRegistry,
    pub forwarding_runtime: &'a tokio::runtime::Handle,
    pub i18n: &'a I18n,
    pub tokens: &'a ThemeTokens,
    pub ai_keys: oxideterm_ai::AiProviderKeyStore,
    pub file_dialog: CloudSyncFileDialog,
    pub effects: Vec<CloudSyncHostEffect>,
}

pub struct CloudSyncLegacyImportStats {
    pub quick_commands: usize,
    pub plugin_settings: usize,
    pub app_settings: bool,
    pub quick_command_errors: usize,
    pub plugin_settings_failed: bool,
    pub app_settings_failed: bool,
}

impl CloudSyncRuntime<'_> {
    fn notify(&self, cx: &mut App) {
        self.cloud_sync.update(cx, |_, cx| cx.notify());
    }

    fn push_cloud_sync_toast(
        &mut self,
        title: String,
        description: Option<String>,
        kind: CloudSyncNoticeKind,
        _: &mut App,
    ) {
        self.effects.push(CloudSyncHostEffect::Notice {
            title,
            description,
            kind,
        });
    }

    fn i18n_replace(&self, key: &str, replacements: &[(&str, String)]) -> String {
        replace(self.i18n, key, replacements)
    }

    fn sources(&self) -> CloudSyncSources<'_> {
        CloudSyncSources {
            cloud_sync: &self.cloud_sync,
            connection_store: self.connection_store,
            settings_store: self.settings_store,
            forwarding_registry: self.forwarding_registry,
        }
    }

    fn invalidate_cloud_sync_snapshot_caches(&self, cx: &App) {
        self.sources().invalidate_cloud_sync_snapshot_caches(cx);
    }
}

fn replace(i18n: &I18n, key: &str, replacements: &[(&str, String)]) -> String {
    let mut text = i18n.t(key);
    for (name, replacement) in replacements {
        text = text.replace(&format!("{{{{{name}}}}}"), replacement);
    }
    text
}
