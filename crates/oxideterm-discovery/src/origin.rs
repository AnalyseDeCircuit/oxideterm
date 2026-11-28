use crate::{DiscoveryLease, DiscoveryService};
use std::{
    fmt,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

#[derive(Clone)]
pub struct DiscoveryOrigin {
    pub source_id: String,
    pub target_id: String,
    pub target_revision: Option<String>,
    catalog: Option<std::sync::Weak<DiscoveryService>>,
    pub epoch: Uuid,
    pub instance_id: String,
    global_route: Option<CancellationToken>,
    directory_generation: Option<std::sync::Weak<()>>,
    directory_issuer: Option<CancellationToken>,
    allow_reuse_on_open: bool,
    open_request: Arc<std::sync::Mutex<Option<Arc<()>>>>,
    endpoint: Option<(String, u16, Option<String>)>,
    validity: CancellationToken,
    source_authority: CancellationToken,
    asset_session: bool,
    session_identity: Option<String>,
    pub reconnect_from: Option<(String, String)>,
    established: Arc<AtomicBool>,
    resources: Arc<std::sync::Mutex<ResourceOwnership>>,
}

#[derive(Default)]
struct InstanceResources {
    owner: Option<Arc<dyn Send + Sync>>,
    lease: Option<DiscoveryLease>,
}

enum ResourceOwnership {
    Pending(InstanceResources),
    Transport(std::sync::Weak<InstanceResources>),
}

impl fmt::Debug for DiscoveryOrigin {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("DiscoveryOrigin")
            .field("instance", &self.instance_id)
            .finish_non_exhaustive()
    }
}
impl PartialEq for DiscoveryOrigin {
    fn eq(&self, other: &Self) -> bool {
        self.instance_id == other.instance_id
    }
}
impl Eq for DiscoveryOrigin {}

impl DiscoveryOrigin {
    pub fn new(source_id: String, target_id: String, epoch: Uuid) -> Self {
        Self {
            source_id,
            target_id,
            target_revision: None,
            catalog: None,
            epoch,
            instance_id: Uuid::new_v4().to_string(),
            endpoint: None,
            global_route: None,
            directory_generation: None,
            directory_issuer: None,
            allow_reuse_on_open: false,
            open_request: Default::default(),
            validity: CancellationToken::new(),
            source_authority: CancellationToken::new(),
            asset_session: false,
            session_identity: None,
            reconnect_from: None,
            established: Default::default(),
            resources: Arc::new(std::sync::Mutex::new(ResourceOwnership::Pending(
                Default::default(),
            ))),
        }
    }
    pub fn with_target_revision(
        mut self,
        catalog: &Arc<DiscoveryService>,
        revision: Option<String>,
    ) -> Self {
        self.catalog = Some(Arc::downgrade(catalog));
        self.directory_generation =
            catalog.directory_target_generation(&self.source_id, self.epoch, &self.target_id);
        self.target_revision = revision;
        self
    }

    fn target_is_current(&self) -> bool {
        !self
            .global_route
            .as_ref()
            .is_some_and(CancellationToken::is_cancelled)
            && self.catalog.as_ref().is_none_or(|catalog| {
                self.directory_generation_current()
                    && catalog.upgrade().is_some_and(|s| {
                        s.is_current_target(
                            &self.source_id,
                            self.epoch,
                            &self.target_id,
                            self.target_revision.as_deref(),
                        )
                    })
            })
    }

    async fn target_changed(&self) {
        let Some(catalog) = &self.catalog else {
            return std::future::pending().await;
        };
        let Some(catalog) = catalog.upgrade() else {
            return;
        };
        let mut changes = catalog.subscribe();
        while self.target_is_current() {
            if changes.changed().await.is_err() {
                return;
            }
        }
    }

    pub fn with_instance(
        mut self,
        id: String,
        validity: CancellationToken,
        owner: Arc<dyn Send + Sync>,
    ) -> Self {
        self.instance_id = id;
        self.validity = validity;
        if let ResourceOwnership::Pending(resources) = &mut *self.resources.lock().unwrap() {
            resources.owner = Some(owner);
        }
        self
    }
    pub fn with_asset_session(mut self, enabled: bool) -> Self {
        self.asset_session = enabled;
        self
    }

    pub fn with_session_identity(mut self, identity: Option<String>) -> Self {
        self.session_identity = identity;
        self
    }

    /// Compare provider-issued identities only within the original catalog grant.
    /// The previous transport and its instance resources may already be gone.
    pub fn same_access_identity(&self, next: &Self) -> Option<bool> {
        if self.source_id != next.source_id
            || self.epoch != next.epoch
            || self.target_id != next.target_id
            || self.target_revision != next.target_revision
            || !self.target_is_current()
            || !next.target_is_current()
            || self.source_authority.is_cancelled()
        {
            return None;
        }
        Some(self.session_identity.as_ref()? == next.session_identity.as_ref()?)
    }

    pub fn is_asset_session(&self) -> bool {
        self.asset_session
    }

    pub fn allows_asset_channels(&self) -> bool {
        self.asset_session && self.consumer_authority_current()
    }

    pub fn with_open_request(self, request: Arc<()>) -> Self {
        *self.open_request.lock().unwrap() = Some(request);
        self
    }

    pub fn finish_open_request(&self) {
        self.open_request.lock().unwrap().take();
    }

    pub fn with_directory_reuse(mut self, allowed: bool, issuer: CancellationToken) -> Self {
        self.allow_reuse_on_open = allowed;
        self.directory_issuer = Some(issuer);
        self
    }

    fn directory_generation_current(&self) -> bool {
        self.directory_generation
            .as_ref()
            .is_some_and(|generation| generation.strong_count() > 0)
    }

    /// A non-reusable instance may still represent a different account for the same target.
    pub fn directory_candidate_current(&self) -> bool {
        self.catalog.is_some()
            && !self
                .directory_issuer
                .as_ref()
                .is_some_and(CancellationToken::is_cancelled)
            && self.consumer_authority_current()
    }

    pub fn allows_directory_reuse(&self) -> bool {
        self.allow_reuse_on_open && self.directory_candidate_current()
    }

    pub fn with_global_proxy(
        mut self,
        proxy: &Option<oxideterm_settings::SettingsUpstreamProxyConfig>,
    ) -> Self {
        if let Some(catalog) = &self.catalog {
            self.global_route = Some(
                catalog
                    .upgrade()
                    .and_then(|catalog| catalog.global_proxy_authority(proxy))
                    .unwrap_or_else(|| {
                        let token = CancellationToken::new();
                        token.cancel();
                        token
                    }),
            );
        }
        self
    }

    pub fn consumer_authority_current(&self) -> bool {
        self.established.load(Ordering::Acquire)
            && self.usable()
            && !self.source_authority.is_cancelled()
            && self.target_is_current()
    }

    pub fn allows_target_operations(&self) -> bool {
        self.usable() && self.is_established()
    }
    pub async fn binding_changed(&self) {
        tokio::select! {
            _ = self.target_changed() => {},
            _ = async {
                match &self.global_route {
                    Some(token) => token.cancelled().await,
                    None => std::future::pending().await,
                }
            } => {},
        }
    }

    pub fn with_endpoint(mut self, host: String, port: u16, username: Option<String>) -> Self {
        self.endpoint = Some((host, port, username));
        self
    }
    pub fn matches_endpoint(&self, host: &str, port: u16, username: &str) -> bool {
        self.endpoint.as_ref().is_none_or(|(h, p, u)| {
            h == host && *p == port && u.as_ref().is_none_or(|u| u == username)
        })
    }
    pub fn host_authorized(&mut self, lease: DiscoveryLease, authority: CancellationToken) {
        if let ResourceOwnership::Pending(resources) = &mut *self.resources.lock().unwrap() {
            resources.lease = Some(lease);
        }
        self.source_authority = authority;
    }

    /// Transfer the open request's resource ownership to the physical connections. Node
    /// metadata and retained terminal snapshots can outlive those connections without pinning them.
    pub fn claim_transport_ownership(&self) -> Option<Arc<dyn std::any::Any + Send + Sync>> {
        let mut ownership = self.resources.lock().unwrap();
        let guard = match &mut *ownership {
            ResourceOwnership::Pending(resources) => {
                if resources.lease.is_none() && resources.owner.is_none() {
                    return None;
                }
                let guard = Arc::new(std::mem::take(resources));
                *ownership = ResourceOwnership::Transport(Arc::downgrade(&guard));
                guard
            }
            ResourceOwnership::Transport(resources) => resources.upgrade()?,
        };
        Some(guard)
    }
    pub fn usable(&self) -> bool {
        !self.validity.is_cancelled()
            && match &*self.resources.lock().unwrap() {
                ResourceOwnership::Pending(_) => true,
                ResourceOwnership::Transport(resources) => resources.strong_count() > 0,
            }
    }
    pub fn can_handshake(&self) -> bool {
        self.usable()
            && self.target_is_current()
            && !self.source_authority.is_cancelled()
            && !self.is_established()
    }
    pub fn connected(&self) {
        self.established.store(true, Ordering::Release);
    }
    pub fn is_established(&self) -> bool {
        self.established.load(Ordering::Acquire)
    }
    /// Observe transport handoff without keeping pending credentials, leases or the node alive.
    pub fn observe_establishment(&self) -> std::sync::Weak<AtomicBool> {
        Arc::downgrade(&self.established)
    }
    pub fn invalidation(&self) -> CancellationToken {
        self.validity.clone()
    }
    pub fn source_authority(&self) -> CancellationToken {
        self.source_authority.clone()
    }
}
