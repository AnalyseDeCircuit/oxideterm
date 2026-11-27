use crate::{DiscoveryFailure, DiscoveryPage, DiscoveryProvider, DiscoverySource, DiscoveryTarget};
use std::{
    collections::{BTreeMap, HashMap, HashSet},
    path::{Path, PathBuf},
    sync::{Arc, LazyLock, Mutex, Weak},
    time::{Duration, SystemTime},
};
use tokio::{
    runtime::Runtime,
    sync::{Notify, watch},
    time::{Instant, timeout_at},
};
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

const ENUMERATION_TIMEOUT: Duration = Duration::from_secs(120);
#[cfg(test)]
mod tests;
type Provider = Arc<dyn DiscoveryProvider>;

pub enum ProviderAvailability {
    Active(Arc<dyn DiscoveryProvider>),
    AwaitingActivation,
    Unavailable,
}

/// Every consumer of a discovered instance keeps a clone until it releases its channel.
#[derive(Clone, Debug)]
pub struct DiscoveryLease {
    _ownership: Arc<()>,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct CatalogEntry {
    pub row_id: String,
    pub source_id: String,
    pub target_id: String,
    pub source_name: String,
    pub label: String,
    pub detail: Option<String>,
    pub scope: Option<String>,
    pub tags: Vec<String>,
    pub group_paths: Vec<Vec<crate::DiscoveryGroupSegment>>,
}

impl CatalogEntry {
    pub fn key(&self) -> (String, String) {
        (self.source_id.clone(), self.target_id.clone())
    }
    pub fn subtitle(&self) -> String {
        self.scope.as_ref().map_or_else(
            || self.source_name.clone(),
            |scope| format!("{} · {scope}", self.source_name),
        )
    }
    pub fn display_detail(&self) -> String {
        let mut parts = vec![self.subtitle()];
        parts.extend(self.detail.iter().cloned());
        parts.extend(self.tags.iter().cloned());
        for path in &self.group_paths {
            parts.push(
                path.iter()
                    .map(|segment| segment.label.as_str())
                    .collect::<Vec<_>>()
                    .join(" › "),
            );
        }
        parts.join(" · ")
    }

    pub fn search_text(&self) -> String {
        let mut text = format!("{} {} {}", self.label, self.subtitle(), self.tags.join(" "));
        for term in self.detail.iter().chain(
            self.group_paths
                .iter()
                .flatten()
                .map(|segment| &segment.label),
        ) {
            text.push(' ');
            text.push_str(term);
        }
        text
    }
}

#[derive(Clone, Default)]
pub struct CatalogSnapshot {
    pub targets: Arc<[DiscoveryTarget]>,
    pub complete: bool,
    pub refreshing: bool,
    pub refreshed_at: Option<SystemTime>,
    pub failure: Option<DiscoveryFailure>,
}

/// Held by an open request until its last provider cleanup reply or process termination.
pub struct DiscoveryPendingOpen {
    _owner: watch::Sender<()>,
}

struct Slot {
    source: DiscoverySource,
    provider: Option<Provider>,
    snapshot: CatalogSnapshot,
    directory_targets: HashMap<String, DirectoryTarget>,
    cancel: CancellationToken,
    catalog_authority: CancellationToken,
    authority: CancellationToken,
    pending: Option<watch::Sender<()>>,
    drained: watch::Receiver<()>,
    refresh: Arc<Notify>,
    reschedule: Arc<Notify>,
    suspended: bool,
    epoch: Uuid,
}

#[derive(Default)]
struct DirectoryTarget {
    generation: Arc<()>,
    revision: Option<String>,
}

impl Slot {
    fn queue_refresh(&mut self) -> bool {
        if self.snapshot.refreshing {
            return false;
        }
        self.snapshot.refreshing = true;
        self.refresh.notify_one();
        true
    }
}

impl Drop for Slot {
    fn drop(&mut self) {
        self.cancel.cancel();
    }
}

pub struct DiscoveryService {
    runtime: Arc<Runtime>,
    sources: Mutex<BTreeMap<String, Slot>>,
    leases: Mutex<HashMap<String, Weak<()>>>,
    global_proxy: Mutex<(
        Option<oxideterm_settings::SettingsUpstreamProxyConfig>,
        CancellationToken,
    )>,
    changes: watch::Sender<u64>,
}

static SERVICES: LazyLock<Mutex<HashMap<PathBuf, Weak<DiscoveryService>>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

impl DiscoveryService {
    pub fn shared(path: &Path, runtime: &Arc<Runtime>) -> Arc<Self> {
        let path = path.canonicalize().unwrap_or_else(|_| path.to_owned());
        let mut services = SERVICES.lock().unwrap();
        if let Some(owner) = services.get(&path).and_then(Weak::upgrade) {
            return owner;
        }
        services.retain(|_, owner| owner.strong_count() > 0);
        let service = Arc::new(Self {
            runtime: runtime.clone(),
            sources: Mutex::new(BTreeMap::new()),
            changes: watch::channel(0).0,
            leases: Mutex::new(HashMap::new()),
            global_proxy: Default::default(),
        });
        services.insert(path, Arc::downgrade(&service));
        service
    }

    pub fn subscribe(&self) -> watch::Receiver<u64> {
        self.changes.subscribe()
    }

    pub fn set_global_proxy(
        &self,
        config: &Option<oxideterm_settings::SettingsUpstreamProxyConfig>,
    ) {
        let mut current = self.global_proxy.lock().unwrap();
        if &current.0 != config {
            current.1.cancel();
            *current = (config.clone(), CancellationToken::new());
        }
    }

    pub(crate) fn global_proxy_authority(
        &self,
        config: &Option<oxideterm_settings::SettingsUpstreamProxyConfig>,
    ) -> Option<CancellationToken> {
        let current = self.global_proxy.lock().unwrap();
        (&current.0 == config).then(|| current.1.clone())
    }
    fn changed(&self) {
        self.changes.send_modify(|n| *n = n.wrapping_add(1));
    }

    pub fn source(&self, id: &str) -> Option<DiscoverySource> {
        self.sources
            .lock()
            .unwrap()
            .get(id)
            .map(|slot| slot.source.clone())
    }

    pub fn matches_sources(&self, sources: &[DiscoverySource]) -> bool {
        let slots = self.sources.lock().unwrap();
        slots.len() == sources.len()
            && sources
                .iter()
                .all(|s| slots.get(&s.id).is_some_and(|slot| &slot.source == s))
    }

    pub fn snapshot(&self, id: &str) -> Option<CatalogSnapshot> {
        self.sources
            .lock()
            .unwrap()
            .get(id)
            .map(|slot| slot.snapshot.clone())
    }

    pub fn target_revision(&self, id: &str, target: &str) -> Option<Option<String>> {
        let slots = self.sources.lock().unwrap();
        slots
            .get(id)?
            .snapshot
            .targets
            .iter()
            .find(|t| t.id == target)
            .map(|t| t.target_revision.clone())
    }

    pub fn directory_target_generation(
        &self,
        id: &str,
        epoch: Uuid,
        target: &str,
    ) -> Option<Weak<()>> {
        let slots = self.sources.lock().unwrap();
        let slot = slots.get(id).filter(|s| s.epoch == epoch)?;
        Some(Arc::downgrade(
            &slot.directory_targets.get(target)?.generation,
        ))
    }

    pub(crate) fn is_current_target(
        &self,
        id: &str,
        epoch: Uuid,
        target: &str,
        revision: Option<&str>,
    ) -> bool {
        self.sources.lock().unwrap().get(id).is_some_and(|slot| {
            slot.epoch == epoch
                && slot.source.enabled
                && !slot.suspended
                && slot.provider.is_some()
                && slot
                    .directory_targets
                    .get(target)
                    .is_some_and(|entry| entry.revision.as_deref() == revision)
        })
    }

    pub fn can_open(&self, id: &str, target: &str) -> bool {
        self.sources.lock().unwrap().get(id).is_some_and(|s| {
            s.source.enabled
                && !s.suspended
                && s.provider.is_some()
                && s.snapshot.targets.iter().any(|t| t.id == target)
        })
    }

    pub fn provider_available(&self, id: &str) -> bool {
        self.sources
            .lock()
            .unwrap()
            .get(id)
            .is_some_and(|s| s.provider.is_some())
    }

    pub fn epoch(&self, id: &str) -> Option<Uuid> {
        self.sources
            .lock()
            .unwrap()
            .get(id)
            .filter(|s| s.source.enabled && !s.suspended && s.provider.is_some())
            .map(|s| s.epoch)
    }

    pub fn authority(&self, id: &str, epoch: Uuid) -> Option<CancellationToken> {
        let slots = self.sources.lock().unwrap();
        let slot = slots.get(id)?;
        (slot.epoch == epoch && !slot.suspended && slot.source.enabled)
            .then(|| slot.authority.clone())
    }

    pub fn acquire(&self, id: &str, epoch: Uuid) -> Option<DiscoveryLease> {
        let slots = self.sources.lock().unwrap();
        let slot = slots.get(id)?;
        if slot.epoch != epoch || slot.suspended || !slot.source.enabled {
            return None;
        }
        let mut leases = self.leases.lock().unwrap();
        let ownership = leases.get(id).and_then(Weak::upgrade).unwrap_or_else(|| {
            let value = Arc::new(());
            leases.insert(id.into(), Arc::downgrade(&value));
            value
        });
        Some(DiscoveryLease {
            _ownership: ownership,
        })
    }

    pub fn occupied(&self, id: &str) -> bool {
        self.leases
            .lock()
            .unwrap()
            .get(id)
            .is_some_and(|l| l.strong_count() > 0)
    }

    pub fn track_open(&self, id: &str, epoch: Uuid) -> Option<DiscoveryPendingOpen> {
        let slots = self.sources.lock().unwrap();
        let slot = slots.get(id)?;
        if slot.epoch != epoch || slot.suspended || !slot.source.enabled {
            return None;
        }
        Some(DiscoveryPendingOpen {
            _owner: slot.pending.clone()?,
        })
    }

    pub fn begin_remove(
        &self,
        id: &str,
        settings: &mut oxideterm_settings::SettingsStore,
    ) -> anyhow::Result<Option<watch::Receiver<()>>> {
        let mut slots = self.sources.lock().unwrap();
        if self.occupied(id) {
            return Ok(None);
        }
        let slot = slots
            .get_mut(id)
            .ok_or_else(|| anyhow::anyhow!("Unknown source"))?;
        let mut sources = settings.settings().discovery_sources.clone();
        let record = sources
            .iter_mut()
            .find(|source| source.id == id)
            .ok_or_else(|| anyhow::anyhow!("Unknown source"))?;
        if !record.removal_pending || record.enabled {
            record.enabled = false;
            record.removal_pending = true;
            // Keep acquisition serialized with the durable transition. A failed write must
            // leave active grants, workers and the caller's settings intact.
            settings.replace_discovery_sources(sources)?;
        }
        slot.pending.take();
        slot.directory_targets.clear();
        slot.epoch = Uuid::new_v4();
        slot.suspended = true;
        slot.source.enabled = false;
        slot.source.removal_pending = true;
        slot.snapshot.refreshing = false;
        slot.cancel.cancel();
        let drained = slot.drained.clone();
        drop(slots);
        self.changed();
        Ok(Some(drained))
    }

    pub fn finish_remove(
        &self,
        id: &str,
        settings: &mut oxideterm_settings::SettingsStore,
    ) -> anyhow::Result<()> {
        let Some(source) = settings
            .settings()
            .discovery_sources
            .iter()
            .find(|s| s.id == id)
        else {
            anyhow::ensure!(
                self.source(id).is_none(),
                "Source cleanup record is missing"
            );
            return Ok(());
        };
        anyhow::ensure!(
            source.removal_pending && !source.enabled,
            "Source deletion has not started"
        );
        let drained =
            self.sources.lock().unwrap().get(id).is_some_and(|slot| {
                slot.source.removal_pending && slot.drained.has_changed().is_err()
            });
        anyhow::ensure!(drained, "Provider cleanup is still in progress");
        let mut updated = settings.settings().discovery_sources.clone();
        updated.retain(|s| s.id != id);
        settings.replace_discovery_sources(updated)?;
        self.sources.lock().unwrap().remove(id);
        self.leases.lock().unwrap().remove(id);
        self.changed();
        Ok(())
    }

    pub fn visible_entries(&self) -> Vec<CatalogEntry> {
        let slots = self.sources.lock().unwrap();
        let mut entries = Vec::new();
        for slot in slots
            .values()
            .filter(|s| s.source.enabled && s.provider.is_some())
        {
            entries.extend(slot.snapshot.targets.iter().map(|target| CatalogEntry {
                row_id: format!(
                    "discovery:{}:{}{}",
                    slot.source.id.len(),
                    slot.source.id,
                    target.id
                ),
                source_id: slot.source.id.clone(),
                target_id: target.id.clone(),
                source_name: slot.source.name.clone(),
                label: target.label.clone(),
                detail: target.detail.clone(),
                scope: target.scope.clone(),
                tags: target.tags.clone(),
                group_paths: target.group_paths.clone(),
            }));
        }
        entries
    }

    /// Reconcile each installed declaration with the locally approved fields before scheduling.
    pub fn configure_saved(
        self: &Arc<Self>,
        settings: &oxideterm_settings::SettingsStore,
        mut resolve: impl FnMut(&DiscoverySource) -> ProviderAvailability,
    ) {
        self.set_global_proxy(&settings.settings().network.upstream_proxy);
        let sources = settings
            .settings()
            .discovery_sources
            .iter()
            .map(|source| {
                if source.removal_pending {
                    return (source.clone(), ProviderAvailability::Unavailable);
                }
                (source.clone(), resolve(source))
            })
            .collect();
        self.configure(sources);
    }

    fn configure(self: &Arc<Self>, sources: Vec<(DiscoverySource, ProviderAvailability)>) {
        let ids: HashSet<_> = sources.iter().map(|(s, _)| s.id.clone()).collect();
        let mut slots = self.sources.lock().unwrap();
        let mut changed = false;
        slots.retain(|id, _| {
            let keep = ids.contains(id);
            changed |= !keep;
            keep
        });
        for (mut source, availability) in sources {
            // A window without a running process may still confirm the declaration is approved.
            let approved = !matches!(availability, ProviderAvailability::Unavailable);
            let mut provider = match availability {
                ProviderAvailability::Active(provider) => Some(provider),
                _ => None,
            };
            if source.removal_pending {
                source.enabled = false;
            }
            if let Some(slot) = slots.get_mut(&source.id) {
                if slot.source.removal_pending {
                    continue;
                }
                if slot.source.provider_id == source.provider_id
                    && slot.source.plugin_id == source.plugin_id
                    && slot.provider.as_ref().is_some_and(|p| p.is_live())
                    && approved
                {
                    provider = slot.provider.clone();
                }
                let same_provider = slot.provider.as_ref().map(|p| p.generation())
                    == provider.as_ref().map(|p| p.generation());
                if same_provider && slot.source == source {
                    continue;
                }
                if same_provider
                    && slot.source.plugin_id == source.plugin_id
                    && slot.source.provider_id == source.provider_id
                    && slot.source.enabled == source.enabled
                    && provider
                        .as_ref()
                        .is_none_or(|p| p.catalog_config(&slot.source) == p.catalog_config(&source))
                {
                    let changed_config = slot.source.config != source.config;
                    let changed_open_preferences =
                        slot.source.ssh_preferences != source.ssh_preferences;
                    let changed_interval = slot.source.refresh_minutes != source.refresh_minutes;
                    slot.source = source;
                    if changed_config || changed_open_preferences {
                        slot.epoch = Uuid::new_v4();
                        for target in slot.directory_targets.values_mut() {
                            target.generation = Arc::new(());
                        }
                        slot.authority.cancel();
                        slot.authority = slot.cancel.child_token();
                    }
                    if changed_interval {
                        slot.reschedule.notify_one();
                    }
                    changed = true;
                    continue;
                }
            }
            let cancel = CancellationToken::new();
            // A canceled open may still own provider cleanup across a source reconfiguration.
            let (pending, drained) = slots
                .get(&source.id)
                .map(|old| (old.pending.clone(), old.drained.clone()))
                .unwrap_or_else(|| {
                    let (tx, rx) = watch::channel(());
                    (Some(tx), rx)
                });
            // Replacing the provider or its configuration cannot prove that listing works again.
            let suspended = slots.get(&source.id).is_some_and(|old| old.suspended);
            let failure = slots
                .get(&source.id)
                .filter(|old| old.suspended)
                .and_then(|old| old.snapshot.failure.clone());
            let slot = Slot {
                source,
                provider,
                pending,
                drained,
                catalog_authority: cancel.child_token(),
                authority: cancel.child_token(),
                cancel,
                snapshot: CatalogSnapshot {
                    failure,
                    ..Default::default()
                },
                directory_targets: HashMap::new(),
                suspended,
                refresh: Arc::new(Notify::new()),
                reschedule: Arc::new(Notify::new()),
                epoch: Uuid::new_v4(),
            };
            if slot.source.enabled && slot.provider.is_some() {
                self.start_directory(&slot);
            }
            slots.insert(slot.source.id.clone(), slot);
            changed = true;
        }
        drop(slots);
        if changed {
            self.changed();
        }
    }

    pub fn request_directory(&self, source_id: &str, force: bool) {
        let mut slots = self.sources.lock().unwrap();
        if let Some(slot) = slots.get_mut(source_id)
            && slot.source.enabled
            && slot.provider.is_some()
            && (force
                || (!slot.snapshot.complete
                    && !slot.snapshot.refreshing
                    && !slot.suspended
                    && slot.snapshot.failure.is_none()))
        {
            let changed = slot.queue_refresh();
            drop(slots);
            if changed {
                self.changed();
            }
        }
    }

    pub fn source_action(
        &self,
        source_id: &str,
        action_id: &str,
    ) -> Option<(
        CancellationToken,
        tokio::task::JoinHandle<Result<(), DiscoveryFailure>>,
    )> {
        let slots = self.sources.lock().unwrap();
        let slot = slots.get(source_id)?;
        let provider = slot
            .provider
            .as_ref()
            .filter(|provider| {
                slot.source.enabled
                    && provider.is_live()
                    && provider.supports_source_action(action_id)
            })?
            .clone();
        let source = slot.source.clone();
        let cancel = slot.authority.child_token();
        drop(slots);
        let action_id = action_id.to_owned();
        let request_cancel = cancel.clone();
        let task = self.runtime.spawn(async move {
            provider
                .source_action(&source, &action_id, request_cancel)
                .await
        });
        Some((cancel, task))
    }

    pub fn report_failure(&self, id: &str, epoch: Uuid, error: DiscoveryFailure) {
        let mut slots = self.sources.lock().unwrap();
        if let Some(slot) = slots.get_mut(id)
            && slot.epoch == epoch
        {
            if error.suspends_source() {
                apply_failure(slot, error);
            } else if slot.suspended || !error.target_changed() || !slot.queue_refresh() {
                return;
            }
            drop(slots);
            self.changed();
        }
    }

    pub fn ensure_visible(&self) {
        let sources: Vec<_> = self
            .sources
            .lock()
            .unwrap()
            .values()
            .filter(|s| s.source.enabled && s.provider.is_some())
            .map(|s| s.source.id.clone())
            .collect();
        for id in sources {
            self.request_directory(&id, false);
        }
    }

    fn start_directory(self: &Arc<Self>, slot: &Slot) {
        let owner = Arc::downgrade(self);
        let id = slot.source.id.clone();
        let trigger = slot.refresh.clone();
        let reschedule = slot.reschedule.clone();
        let cancel = slot.cancel.clone();
        self.runtime.spawn(async move {
            loop {
                let Some((source, _, suspended)) = current(&owner, &id, &cancel) else {
                    return;
                };
                let period = source
                    .refresh_minutes
                    .filter(|_| !suspended)
                    .map(|n| Duration::from_secs(u64::from(n.get()) * 60));
                let requested = tokio::select! {
                    biased;
                    _ = cancel.cancelled() => return,
                    _ = trigger.notified() => true,
                    _ = reschedule.notified() => continue,
                    _ = wait_period(period) => false,
                };
                let Some((source, provider, authority)) = owner.upgrade().and_then(|host| {
                    let mut slots = host.sources.lock().unwrap();
                    let slot = slots.get_mut(&id)?;
                    // A timer may have served a queued request before its wakeup was consumed.
                    if cancel.is_cancelled()
                        || (requested && !slot.snapshot.refreshing)
                        || (!requested && slot.suspended)
                    {
                        return None;
                    }
                    let was_refreshing = slot.snapshot.refreshing;
                    slot.snapshot.refreshing = true;
                    let round = (
                        slot.source.clone(),
                        slot.provider.clone()?,
                        slot.catalog_authority.clone(),
                    );
                    drop(slots);
                    if !was_refreshing {
                        host.changed();
                    }
                    Some(round)
                }) else {
                    if cancel.is_cancelled() {
                        return;
                    }
                    continue;
                };
                let result =
                    enumerate(&source, &provider, &authority, Uuid::new_v4().to_string()).await;
                let Some(host) = owner.upgrade() else { return };
                let mut slots = host.sources.lock().unwrap();
                if cancel.is_cancelled() {
                    return;
                }
                let Some(slot) = slots.get_mut(&id) else {
                    return;
                };
                if authority.is_cancelled() {
                    continue;
                }
                slot.snapshot.refreshing = false;
                match result {
                    Ok(listing) => {
                        // A removed target drops its generation even when a later refresh
                        // introduces the same ID. Display-only edits keep the original grant.
                        slot.directory_targets = listing
                            .targets
                            .iter()
                            .map(|entry| {
                                let mut target =
                                    slot.directory_targets.remove(&entry.id).unwrap_or_default();
                                if target.revision != entry.target_revision {
                                    target.generation = Arc::new(());
                                    target.revision.clone_from(&entry.target_revision);
                                }
                                (entry.id.clone(), target)
                            })
                            .collect();
                        slot.snapshot.targets = listing.targets.into();
                        slot.snapshot.complete = true;
                        slot.snapshot.refreshed_at = Some(SystemTime::now());
                        slot.snapshot.failure = None;
                        slot.suspended = false;
                    }
                    Err(failure) => apply_failure(slot, failure),
                }
                drop(slots);
                host.changed();
            }
        });
    }
}

fn current(
    owner: &Weak<DiscoveryService>,
    id: &str,
    cancel: &CancellationToken,
) -> Option<(DiscoverySource, Provider, bool)> {
    let owner = owner.upgrade()?;
    let slots = owner.sources.lock().unwrap();
    let slot = slots.get(id)?;
    if cancel.is_cancelled() {
        return None;
    }
    Some((slot.source.clone(), slot.provider.clone()?, slot.suspended))
}

fn apply_failure(slot: &mut Slot, failure: DiscoveryFailure) {
    if failure.suspends_source() {
        slot.snapshot = CatalogSnapshot::default();
        slot.directory_targets.clear();
        slot.suspended = true;
        slot.epoch = Uuid::new_v4();
        slot.catalog_authority.cancel();
        slot.catalog_authority = slot.cancel.child_token();
        slot.authority.cancel();
        slot.authority = slot.cancel.child_token();
    }
    slot.snapshot.failure = Some(failure);
}

async fn wait_period(period: Option<Duration>) {
    match period {
        Some(delay) => tokio::time::sleep(delay).await,
        None => std::future::pending().await,
    }
}

async fn enumerate(
    source: &DiscoverySource,
    provider: &Provider,
    cancel: &CancellationToken,
    refresh_id: String,
) -> Result<DiscoveryPage, DiscoveryFailure> {
    let deadline = Instant::now() + ENUMERATION_TIMEOUT;
    let pages = async {
        let mut cursors = HashSet::new();
        let mut targets = BTreeMap::new();
        let mut cursor = None;
        loop {
            let listing = provider.list(source, &refresh_id, cursor, cancel).await?;
            let DiscoveryPage {
                targets: page,
                next_cursor,
            } = listing;
            for target in page {
                if target.id.is_empty() || target.label.is_empty() {
                    return Err(DiscoveryFailure::InvalidResponse);
                }
                if let Some(previous) = targets.insert(target.id.clone(), target.clone())
                    && previous != target
                {
                    return Err(DiscoveryFailure::InvalidResponse);
                }
            }
            match next_cursor {
                None => {
                    return Ok(DiscoveryPage {
                        targets: targets.into_values().collect(),
                        next_cursor: None,
                    });
                }
                Some(next) if !next.is_empty() && cursors.insert(next.clone()) => {
                    cursor = Some(next)
                }
                Some(_) => return Err(DiscoveryFailure::InvalidResponse),
            }
            // A provider can immediately produce another page without yielding to Tokio's timer.
            if Instant::now() >= deadline {
                return Err(DiscoveryFailure::Unavailable);
            }
            tokio::task::yield_now().await;
        }
    };
    let result = tokio::select! {
        biased;
        _=cancel.cancelled()=>Err(DiscoveryFailure::Unavailable),
        r=timeout_at(deadline,pages)=>r.unwrap_or(Err(DiscoveryFailure::Unavailable)),
    };
    if result.is_err() {
        provider.cancel_list(source, &refresh_id).await;
    }
    result
}
