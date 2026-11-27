//! Device-local source scheduling and publication of complete discovery snapshots.

mod catalog;
mod origin;
pub use origin::DiscoveryOrigin;

pub use catalog::{
    CatalogEntry, CatalogSnapshot, DiscoveryLease, DiscoveryPendingOpen, DiscoveryService,
};
pub use oxideterm_plugin_protocol::discovery::{
    DiscoveryGroupSegment, DiscoveryPage, DiscoveryTarget,
};
pub use oxideterm_settings::DiscoverySource;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DiscoveryFailure {
    Unavailable,
    PermissionDenied,
    StaleTarget,
    InvalidResponse,
    Provider(oxideterm_plugin_protocol::PluginError),
}

impl DiscoveryFailure {
    pub fn suspends_source(&self) -> bool {
        matches!(self, Self::Provider(error) if error.code == "sourceAccessLost")
    }

    fn target_changed(&self) -> bool {
        matches!(self, Self::StaleTarget)
            || matches!(self, Self::Provider(error) if matches!(error.code.as_str(), "targetGone" | "staleTarget"))
    }
}

#[async_trait::async_trait]
pub trait DiscoveryProvider: Send + Sync {
    fn is_live(&self) -> bool {
        true
    }
    fn generation(&self) -> u64;
    fn catalog_config(&self, source: &DiscoverySource) -> serde_json::Value {
        source.config.clone()
    }
    fn supports_source_action(&self, _action_id: &str) -> bool {
        false
    }
    async fn source_action(
        &self,
        _source: &DiscoverySource,
        _action_id: &str,
        _cancel: tokio_util::sync::CancellationToken,
    ) -> Result<(), DiscoveryFailure> {
        Err(DiscoveryFailure::Unavailable)
    }
    async fn list(
        &self,
        source: &DiscoverySource,
        refresh_id: &str,
        cursor: Option<String>,
        cancel: &tokio_util::sync::CancellationToken,
    ) -> Result<DiscoveryPage, DiscoveryFailure>;
    async fn cancel_list(&self, source: &DiscoverySource, refresh_id: &str);
}
