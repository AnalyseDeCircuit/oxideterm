// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use super::*;
use std::sync::Weak;

const FOREGROUND_INTERVAL: Duration = Duration::from_secs(5);
const BACKGROUND_INTERVAL: Duration = Duration::from_secs(15);
const VISIBLE_LEASE: Duration = Duration::from_secs(10);
const SAMPLE_TIMEOUT: Duration = Duration::from_secs(3);
const SAMPLE_MAX_AGE: Duration = Duration::from_secs(30);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SshLatencySnapshot {
    Pending,
    Measured { millis: u64, measured_at: Instant },
    Timeout,
    Unavailable,
    Unsupported,
    Stale,
}

struct LatencyEntry {
    transport: Weak<PooledSshConnection>,
    sample: Arc<RwLock<SshLatencySnapshot>>,
    started_at: Option<Instant>,
    task: Option<tokio::task::JoinHandle<()>>,
    published: SshLatencySnapshot,
}

impl Drop for LatencyEntry {
    fn drop(&mut self) {
        if let Some(task) = self.task.take() {
            task.abort();
        }
    }
}

/// Owned by the workspace runtime, never by an individual terminal consumer.
#[derive(Default)]
pub struct SshLatencyMonitor {
    entries: HashMap<String, LatencyEntry>,
    visible: HashMap<String, Instant>,
}

impl SshLatencyMonitor {
    pub fn mark_visible(&mut self, connection_id: &str) {
        self.visible
            .insert(connection_id.to_owned(), Instant::now());
    }

    pub fn clear(&mut self) {
        self.entries.clear();
        self.visible.clear();
    }

    pub fn snapshot(&self, handle: &SshConnectionHandle) -> SshLatencySnapshot {
        if !matches!(
            handle.state(),
            ConnectionState::Active | ConnectionState::Idle
        ) {
            return SshLatencySnapshot::Unavailable;
        }
        if handle
            .config()
            .ssh_channel_strategy
            .requires_dedicated_consumers()
        {
            return SshLatencySnapshot::Unsupported;
        }
        let Some(transport) = handle.physical::<PooledSshConnection>() else {
            return SshLatencySnapshot::Unavailable;
        };
        if transport.target.is_closed() {
            return SshLatencySnapshot::Unavailable;
        }
        let Some(entry) = self.entries.get(handle.connection_id()) else {
            return SshLatencySnapshot::Pending;
        };
        // Reconnect may replace the transport without replacing its registry id.
        if !Weak::ptr_eq(&entry.transport, &Arc::downgrade(&transport)) {
            return SshLatencySnapshot::Pending;
        }
        let sample = *entry.sample.read();
        match sample {
            SshLatencySnapshot::Measured { measured_at, .. }
                if measured_at.elapsed() > SAMPLE_MAX_AGE =>
            {
                SshLatencySnapshot::Stale
            }
            _ => sample,
        }
    }

    pub fn refresh(
        &mut self,
        registry: &SshConnectionRegistry,
        runtime: &tokio::runtime::Handle,
    ) -> bool {
        let now = Instant::now();
        let previous_count = self.entries.len();
        let mut changed = false;
        let mut retained = HashSet::new();
        for info in registry.list() {
            if !matches!(info.state, ConnectionState::Active | ConnectionState::Idle)
                || !info
                    .consumers
                    .iter()
                    .any(|c| matches!(c, ConnectionConsumer::Terminal(_)))
            {
                continue;
            }
            let Some(handle) = registry.get(&info.connection_id) else {
                continue;
            };
            if handle
                .config()
                .ssh_channel_strategy
                .requires_dedicated_consumers()
            {
                continue;
            }
            let Some(transport) = handle.physical::<PooledSshConnection>() else {
                continue;
            };
            if transport.target.is_closed() {
                continue;
            }
            retained.insert(info.connection_id.clone());
            let identity = Arc::downgrade(&transport);
            if self
                .entries
                .get(&info.connection_id)
                .is_some_and(|e| !Weak::ptr_eq(&e.transport, &identity))
            {
                self.entries.remove(&info.connection_id);
                changed = true;
            }
            let entry = self
                .entries
                .entry(info.connection_id.clone())
                .or_insert_with(|| LatencyEntry {
                    transport: identity,
                    sample: Arc::new(RwLock::new(SshLatencySnapshot::Pending)),
                    started_at: None,
                    task: None,
                    published: SshLatencySnapshot::Pending,
                });
            let sample = *entry.sample.read();
            if sample != entry.published {
                entry.published = sample;
                changed = true;
            }
            let interval = if self
                .visible
                .get(&info.connection_id)
                .is_some_and(|at| now.duration_since(*at) < VISIBLE_LEASE)
            {
                FOREGROUND_INTERVAL
            } else {
                BACKGROUND_INTERVAL
            };
            if entry.task.as_ref().is_some_and(|task| !task.is_finished())
                || entry
                    .started_at
                    .is_some_and(|at| now.duration_since(at) < interval)
            {
                continue;
            }
            entry.started_at = Some(now);
            let sample = entry.sample.clone();
            entry.task = Some(runtime.spawn(async move {
                measure_reply(
                    async {
                        // Either protocol success or rejection proves the peer replied. Unlike
                        // send_keepalive, this API also reports a closed reply channel as an error.
                        match transport
                            .target
                            .send_global_request("keepalive@openssh.com", &[], true)
                            .await
                        {
                            Ok(_) | Err(russh::Error::RequestDenied) => Ok(()),
                            Err(error) => Err(error),
                        }
                    },
                    sample,
                )
                .await;
            }));
        }
        self.entries.retain(|id, _| retained.contains(id));
        self.visible
            .retain(|id, at| retained.contains(id) && now.duration_since(*at) < VISIBLE_LEASE);
        changed || previous_count != self.entries.len()
    }
}

async fn measure_reply(
    reply: impl Future<Output = Result<(), russh::Error>>,
    sample: Arc<RwLock<SshLatencySnapshot>>,
) {
    let started = Instant::now();
    tokio::pin!(reply);
    let result = match timeout(SAMPLE_TIMEOUT, &mut reply).await {
        Ok(result) => result,
        Err(_) => {
            *sample.write() = SshLatencySnapshot::Timeout;
            // Keep at most one unanswered request per transport. Dropping and retrying
            // would accumulate global-request reply slots on silent servers.
            reply.await
        }
    };
    *sample.write() = match result {
        Ok(()) => SshLatencySnapshot::Measured {
            millis: (started.elapsed().as_millis() as u64).max(1),
            measured_at: Instant::now(),
        },
        Err(_) => SshLatencySnapshot::Unavailable,
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn live_transport_shares_samples_and_discards_retired_results() {
        struct Peer;
        impl russh::server::Handler for Peer {
            type Error = russh::Error;
            async fn auth_none(&mut self, _: &str) -> Result<russh::server::Auth, Self::Error> {
                Ok(russh::server::Auth::Accept)
            }
        }
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let key =
            russh::keys::PrivateKey::random(&mut rand10::rng(), russh::keys::Algorithm::Ed25519)
                .unwrap();
        let fingerprint = key
            .public_key()
            .fingerprint(russh::keys::HashAlg::Sha256)
            .to_string();
        let server = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let session = russh::server::run_stream(
                Arc::new(russh::server::Config {
                    keys: vec![key],
                    ..Default::default()
                }),
                stream,
                Peer,
            )
            .await
            .unwrap();
            let _ = session.await;
        });
        let registry = SshConnectionRegistry::default();
        let mut config = SshConfig::password("127.0.0.1", port, "fixture", "synthetic-password");
        config.expected_host_key_fingerprint = Some(fingerprint);
        config.trust_host_key = Some(false);
        let handle = timeout(
            Duration::from_secs(5),
            SshTransportClient::new(config.clone()).connect_node_with_registry(
                registry.clone(),
                ConnectionConsumer::Terminal("first".into()),
            ),
        )
        .await
        .unwrap()
        .unwrap();
        registry.mark_state(handle.connection_id(), ConnectionState::Active);
        let second = registry.acquire(config, ConnectionConsumer::Terminal("second".into()));
        assert_eq!(second.connection_id(), handle.connection_id());
        let mut monitor = SshLatencyMonitor::default();
        monitor.mark_visible(handle.connection_id());
        monitor.refresh(&registry, &tokio::runtime::Handle::current());
        let started = monitor.entries[handle.connection_id()].started_at;
        monitor.refresh(&registry, &tokio::runtime::Handle::current());
        assert_eq!(monitor.entries[handle.connection_id()].started_at, started);
        let task = monitor
            .entries
            .get_mut(handle.connection_id())
            .unwrap()
            .task
            .take()
            .unwrap();
        timeout(Duration::from_secs(5), task)
            .await
            .unwrap()
            .unwrap();
        let sample = monitor.snapshot(&handle);
        assert!(matches!(
            sample,
            SshLatencySnapshot::Measured {
                millis: 1..=3000,
                ..
            }
        ));
        assert_eq!(monitor.snapshot(&second), sample);

        *monitor.entries[handle.connection_id()].sample.write() = SshLatencySnapshot::Measured {
            millis: 128,
            measured_at: Instant::now() - Duration::from_secs(31),
        };
        assert_eq!(monitor.snapshot(&handle), SshLatencySnapshot::Stale);
        handle.clear_physical().await;
        assert_eq!(monitor.snapshot(&handle), SshLatencySnapshot::Unavailable);
        monitor.refresh(&registry, &tokio::runtime::Handle::current());
        assert!(monitor.entries.is_empty());
        server.abort();
    }

    #[tokio::test]
    async fn dedicated_devices_do_not_start_latency_probes() {
        let registry = SshConnectionRegistry::default();
        let mut config = SshConfig::password("device.example", 22, "fixture", "synthetic-password");
        config.ssh_channel_strategy =
            oxideterm_connections::SshChannelStrategy::DedicatedPerConsumer;
        let handle = registry.acquire(config, ConnectionConsumer::Terminal("fixture".into()));
        registry.mark_state(handle.connection_id(), ConnectionState::Active);
        let mut monitor = SshLatencyMonitor::default();
        monitor.refresh(&registry, &tokio::runtime::Handle::current());
        assert_eq!(monitor.snapshot(&handle), SshLatencySnapshot::Unsupported);
        assert!(monitor.entries.is_empty());
    }

    #[tokio::test(start_paused = true)]
    async fn latency_waits_for_reply_and_recovers_after_timeout() {
        let sample = Arc::new(RwLock::new(SshLatencySnapshot::Pending));
        let (send, receive) = tokio::sync::oneshot::channel();
        let task = tokio::spawn(measure_reply(
            async move { receive.await.map_err(|_| russh::Error::Disconnect) },
            sample.clone(),
        ));
        tokio::task::yield_now().await;
        tokio::time::advance(Duration::from_millis(128)).await;
        assert_eq!(*sample.read(), SshLatencySnapshot::Pending);
        send.send(()).unwrap();
        task.await.unwrap();
        assert!(matches!(
            *sample.read(),
            SshLatencySnapshot::Measured { millis: 128, .. }
        ));

        let (send, receive) = tokio::sync::oneshot::channel();
        let task = tokio::spawn(measure_reply(
            async move { receive.await.map_err(|_| russh::Error::Disconnect) },
            sample.clone(),
        ));
        tokio::task::yield_now().await;
        tokio::time::advance(SAMPLE_TIMEOUT).await;
        tokio::task::yield_now().await;
        assert_eq!(*sample.read(), SshLatencySnapshot::Timeout);
        assert!(!task.is_finished());
        drop(send);
        task.await.unwrap();
        assert_eq!(*sample.read(), SshLatencySnapshot::Unavailable);
    }

    #[tokio::test]
    async fn entry_drop_cancels_unanswered_probe() {
        let (send, receive) = tokio::sync::oneshot::channel();
        let mut entry = LatencyEntry {
            transport: Weak::new(),
            sample: Arc::new(RwLock::new(SshLatencySnapshot::Pending)),
            started_at: None,
            task: None,
            published: SshLatencySnapshot::Pending,
        };
        entry.task = Some(tokio::spawn(measure_reply(
            async move { receive.await.map_err(|_| russh::Error::Disconnect) },
            entry.sample.clone(),
        )));
        tokio::task::yield_now().await;
        drop(entry);
        tokio::task::yield_now().await;
        assert!(send.send(()).is_err());
    }
}
