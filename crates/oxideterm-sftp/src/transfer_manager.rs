// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use std::{
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    time::{SystemTime, UNIX_EPOCH},
};

use parking_lot::RwLock;
use std::collections::HashMap;
use tokio::sync::{Notify, Semaphore, watch};

use serde::{Deserialize, Serialize};

use crate::{SftpError, TransferStrategy};

pub const DEFAULT_SFTP_CONCURRENT_TRANSFERS: usize = 3;
pub const DEFAULT_SFTP_DIRECTORY_PARALLELISM: usize = 4;
pub const MAX_SFTP_CONCURRENT_TRANSFERS: usize = 10;
pub const MAX_SFTP_DIRECTORY_PARALLELISM: usize = 16;
const FINISHED_BACKGROUND_TRANSFER_RETENTION_MS: u64 = 5 * 60 * 1000;

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or_default()
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum BackgroundTransferDirection {
    Upload,
    Download,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum BackgroundTransferKind {
    File,
    Directory,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum BackgroundTransferState {
    Pending,
    Active,
    Paused,
    Completed,
    Cancelled,
    Error,
}

impl BackgroundTransferState {
    fn is_finished(self) -> bool {
        matches!(self, Self::Completed | Self::Cancelled | Self::Error)
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BackgroundTransferSnapshot {
    pub id: String,
    pub node_id: String,
    pub name: String,
    pub local_path: String,
    pub remote_path: String,
    pub direction: BackgroundTransferDirection,
    pub kind: BackgroundTransferKind,
    pub strategy: TransferStrategy,
    pub state: BackgroundTransferState,
    pub size: u64,
    pub transferred: u64,
    pub backend_speed: Option<u64>,
    pub error: Option<String>,
    pub start_time: u64,
    pub end_time: Option<u64>,
    pub item_count: Option<u64>,
}

impl BackgroundTransferSnapshot {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        id: String,
        node_id: String,
        name: String,
        local_path: String,
        remote_path: String,
        direction: BackgroundTransferDirection,
        kind: BackgroundTransferKind,
        strategy: TransferStrategy,
        size: u64,
        transferred: u64,
    ) -> Self {
        Self {
            id,
            node_id,
            name,
            local_path,
            remote_path,
            direction,
            kind,
            strategy,
            state: BackgroundTransferState::Pending,
            size,
            transferred,
            backend_speed: None,
            error: None,
            start_time: now_ms(),
            end_time: None,
            item_count: None,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SftpTransferRuntimeSettings {
    pub max_concurrent_transfers: usize,
    pub speed_limit_kbps: usize,
    pub directory_parallelism: usize,
}

impl Default for SftpTransferRuntimeSettings {
    fn default() -> Self {
        Self {
            max_concurrent_transfers: DEFAULT_SFTP_CONCURRENT_TRANSFERS,
            speed_limit_kbps: 0,
            directory_parallelism: DEFAULT_SFTP_DIRECTORY_PARALLELISM,
        }
    }
}

#[derive(Debug)]
pub struct SftpTransferPermit {
    _permit: tokio::sync::OwnedSemaphorePermit,
    active_count: Arc<AtomicUsize>,
    availability_notify: Arc<Notify>,
}

impl Drop for SftpTransferPermit {
    fn drop(&mut self) {
        let _ = self
            .active_count
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |count| {
                Some(count.saturating_sub(1))
            });
        self.availability_notify.notify_waiters();
    }
}

#[derive(Debug)]
pub struct SftpTransferControl {
    cancel_tx: watch::Sender<bool>,
    cancel_rx: watch::Receiver<bool>,
    pause_tx: watch::Sender<bool>,
    pause_rx: watch::Receiver<bool>,
}

impl SftpTransferControl {
    pub fn new() -> Self {
        let (cancel_tx, cancel_rx) = watch::channel(false);
        let (pause_tx, pause_rx) = watch::channel(false);
        Self {
            cancel_tx,
            cancel_rx,
            pause_tx,
            pause_rx,
        }
    }

    pub fn is_cancelled(&self) -> bool {
        *self.cancel_rx.borrow()
    }

    pub fn is_paused(&self) -> bool {
        *self.pause_rx.borrow()
    }

    pub fn cancel(&self) {
        let _ = self.cancel_tx.send(true);
    }

    pub fn pause(&self) {
        let _ = self.pause_tx.send(true);
    }

    pub fn resume(&self) {
        let _ = self.pause_tx.send(false);
    }

    pub fn subscribe_cancellation(&self) -> watch::Receiver<bool> {
        self.cancel_rx.clone()
    }

    pub fn subscribe_pause(&self) -> watch::Receiver<bool> {
        self.pause_rx.clone()
    }
}

impl Default for SftpTransferControl {
    fn default() -> Self {
        Self::new()
    }
}

pub struct SftpTransferGuard {
    manager: Option<Arc<SftpTransferManager>>,
    transfer_id: String,
}

impl SftpTransferGuard {
    pub fn new(manager: Option<&Arc<SftpTransferManager>>, transfer_id: impl Into<String>) -> Self {
        Self {
            manager: manager.cloned(),
            transfer_id: transfer_id.into(),
        }
    }
}

impl Drop for SftpTransferGuard {
    fn drop(&mut self) {
        if let Some(manager) = &self.manager {
            manager.unregister(&self.transfer_id);
        }
    }
}

#[derive(Debug)]
pub struct SftpTransferManager {
    semaphore: Arc<Semaphore>,
    controls: RwLock<HashMap<String, Arc<SftpTransferControl>>>,
    active_count: Arc<AtomicUsize>,
    max_concurrent_transfers: AtomicUsize,
    directory_parallelism: AtomicUsize,
    speed_limit_bps: AtomicUsize,
    availability_notify: Arc<Notify>,
    background_transfers: RwLock<HashMap<String, BackgroundTransferSnapshot>>,
    background_notify: Arc<Notify>,
}

impl SftpTransferManager {
    pub fn new() -> Self {
        Self {
            semaphore: Arc::new(Semaphore::new(MAX_SFTP_CONCURRENT_TRANSFERS)),
            controls: RwLock::new(HashMap::new()),
            active_count: Arc::new(AtomicUsize::new(0)),
            max_concurrent_transfers: AtomicUsize::new(DEFAULT_SFTP_CONCURRENT_TRANSFERS),
            directory_parallelism: AtomicUsize::new(DEFAULT_SFTP_DIRECTORY_PARALLELISM),
            speed_limit_bps: AtomicUsize::new(0),
            availability_notify: Arc::new(Notify::new()),
            background_transfers: RwLock::new(HashMap::new()),
            background_notify: Arc::new(Notify::new()),
        }
    }

    fn cleanup_background_transfers(&self) {
        let now = now_ms();
        self.background_transfers.write().retain(|_, snapshot| {
            !snapshot.state.is_finished()
                || snapshot
                    .end_time
                    .map(|end| now.saturating_sub(end) <= FINISHED_BACKGROUND_TRANSFER_RETENTION_MS)
                    .unwrap_or(true)
        });
    }

    pub fn apply_settings(&self, settings: SftpTransferRuntimeSettings) {
        self.set_max_concurrent(settings.max_concurrent_transfers);
        self.set_speed_limit_kbps(settings.speed_limit_kbps);
        self.set_directory_parallelism(settings.directory_parallelism);
    }

    pub fn set_max_concurrent(&self, max: usize) {
        let clamped = max.clamp(1, MAX_SFTP_CONCURRENT_TRANSFERS);
        self.max_concurrent_transfers
            .store(clamped, Ordering::Release);
        self.availability_notify.notify_waiters();
    }

    pub fn max_concurrent(&self) -> usize {
        self.max_concurrent_transfers.load(Ordering::Acquire)
    }

    pub fn set_directory_parallelism(&self, parallelism: usize) {
        let clamped = parallelism.clamp(1, MAX_SFTP_DIRECTORY_PARALLELISM);
        self.directory_parallelism.store(clamped, Ordering::Release);
    }

    pub fn directory_parallelism(&self) -> usize {
        self.directory_parallelism.load(Ordering::Acquire)
    }

    pub fn set_speed_limit_kbps(&self, kbps: usize) {
        self.speed_limit_bps
            .store(kbps.saturating_mul(1024), Ordering::Release);
    }

    pub fn speed_limit_bps(&self) -> usize {
        self.speed_limit_bps.load(Ordering::Acquire)
    }

    pub fn active_count(&self) -> usize {
        self.active_count.load(Ordering::Acquire)
    }

    pub fn registered_count(&self) -> usize {
        self.controls.read().len()
    }

    pub fn register(&self, transfer_id: &str) -> Arc<SftpTransferControl> {
        let control = Arc::new(SftpTransferControl::new());
        self.controls
            .write()
            .insert(transfer_id.to_string(), control.clone());
        control
    }

    pub fn get_control(&self, transfer_id: &str) -> Option<Arc<SftpTransferControl>> {
        self.controls.read().get(transfer_id).cloned()
    }

    pub fn unregister(&self, transfer_id: &str) {
        self.controls.write().remove(transfer_id);
    }

    pub fn register_background_transfer(&self, mut snapshot: BackgroundTransferSnapshot) {
        self.cleanup_background_transfers();
        // Match Tauri: callers may seed a speculative state, but registration
        // always exposes a queued background transfer until the task starts.
        snapshot.state = BackgroundTransferState::Pending;
        self.background_transfers
            .write()
            .insert(snapshot.id.clone(), snapshot);
        self.background_notify.notify_waiters();
    }

    pub fn update_background_transfer_strategy(
        &self,
        transfer_id: &str,
        strategy: TransferStrategy,
    ) {
        if let Some(snapshot) = self.background_transfers.write().get_mut(transfer_id) {
            snapshot.strategy = strategy;
            self.background_notify.notify_waiters();
        }
    }

    pub fn mark_background_transfer_active(&self, transfer_id: &str) {
        if let Some(snapshot) = self.background_transfers.write().get_mut(transfer_id) {
            snapshot.state = BackgroundTransferState::Active;
            self.background_notify.notify_waiters();
        }
    }

    pub fn update_background_transfer_progress(
        &self,
        transfer_id: &str,
        transferred: u64,
        total: u64,
        speed: u64,
    ) {
        if let Some(snapshot) = self.background_transfers.write().get_mut(transfer_id) {
            snapshot.transferred = transferred;
            if total > 0 {
                snapshot.size = total;
            }
            snapshot.backend_speed = Some(speed);
            if !snapshot.state.is_finished() {
                snapshot.state = BackgroundTransferState::Active;
            }
            self.background_notify.notify_waiters();
        }
    }

    pub fn finish_background_transfer(
        &self,
        transfer_id: &str,
        state: BackgroundTransferState,
        error: Option<String>,
        item_count: Option<u64>,
    ) -> Option<BackgroundTransferSnapshot> {
        let mut transfers = self.background_transfers.write();
        let snapshot = transfers.get_mut(transfer_id)?;
        snapshot.state = state;
        snapshot.error = error;
        snapshot.item_count = item_count;
        snapshot.end_time = Some(now_ms());
        if state == BackgroundTransferState::Completed && snapshot.size > 0 {
            snapshot.transferred = snapshot.size;
        }
        let snapshot = snapshot.clone();
        drop(transfers);
        self.background_notify.notify_waiters();
        Some(snapshot)
    }

    pub fn get_background_transfer(&self, transfer_id: &str) -> Option<BackgroundTransferSnapshot> {
        self.cleanup_background_transfers();
        self.background_transfers.read().get(transfer_id).cloned()
    }

    pub fn list_background_transfers(
        &self,
        node_id: Option<&str>,
    ) -> Vec<BackgroundTransferSnapshot> {
        self.cleanup_background_transfers();
        let mut snapshots: Vec<_> = self
            .background_transfers
            .read()
            .values()
            .filter(|snapshot| node_id.is_none_or(|id| snapshot.node_id == id))
            .cloned()
            .collect();
        snapshots.sort_by_key(|snapshot| snapshot.start_time);
        snapshots
    }

    pub async fn wait_background_transfer_finished(
        &self,
        transfer_id: &str,
    ) -> Option<BackgroundTransferSnapshot> {
        loop {
            let notified = self.background_notify.notified();
            match self.get_background_transfer(transfer_id) {
                Some(snapshot) if snapshot.state.is_finished() => return Some(snapshot),
                Some(_) => notified.await,
                None => return None,
            }
        }
    }

    pub fn cancel(&self, transfer_id: &str) -> bool {
        if let Some(control) = self.get_control(transfer_id) {
            control.cancel();
            true
        } else {
            false
        }
    }

    pub fn pause(&self, transfer_id: &str) -> bool {
        if let Some(control) = self.get_control(transfer_id) {
            control.pause();
            true
        } else {
            false
        }
    }

    pub fn resume(&self, transfer_id: &str) -> bool {
        if let Some(control) = self.get_control(transfer_id) {
            control.resume();
            true
        } else {
            false
        }
    }

    pub fn cancel_all(&self) {
        for control in self.controls.read().values() {
            control.cancel();
        }
    }

    pub async fn check_control(&self, transfer_id: &str) -> Result<(), SftpError> {
        let Some(control) = self.get_control(transfer_id) else {
            return Ok(());
        };
        if control.is_cancelled() {
            return Err(SftpError::TransferCancelled);
        }
        while control.is_paused() {
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
            if control.is_cancelled() {
                return Err(SftpError::TransferCancelled);
            }
        }
        Ok(())
    }

    pub async fn acquire_permit(&self) -> SftpTransferPermit {
        loop {
            let notified = self.availability_notify.notified();
            if self.active_count() < self.max_concurrent() {
                break;
            }
            notified.await;
        }

        let permit = self
            .semaphore
            .clone()
            .acquire_owned()
            .await
            .expect("SFTP transfer semaphore should stay open for app lifetime");
        self.active_count.fetch_add(1, Ordering::AcqRel);
        SftpTransferPermit {
            _permit: permit,
            active_count: self.active_count.clone(),
            availability_notify: self.availability_notify.clone(),
        }
    }
}

impl Default for SftpTransferManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn applies_tauri_sftp_transfer_settings() {
        let manager = SftpTransferManager::new();
        manager.apply_settings(SftpTransferRuntimeSettings {
            max_concurrent_transfers: 5,
            speed_limit_kbps: 256,
            directory_parallelism: 8,
        });

        assert_eq!(manager.max_concurrent(), 5);
        assert_eq!(manager.speed_limit_bps(), 256 * 1024);
        assert_eq!(manager.directory_parallelism(), 8);
    }

    #[test]
    fn clamps_like_tauri_backend_command() {
        let manager = SftpTransferManager::new();
        manager.apply_settings(SftpTransferRuntimeSettings {
            max_concurrent_transfers: 99,
            speed_limit_kbps: 0,
            directory_parallelism: 99,
        });

        assert_eq!(manager.max_concurrent(), MAX_SFTP_CONCURRENT_TRANSFERS);
        assert_eq!(
            manager.directory_parallelism(),
            MAX_SFTP_DIRECTORY_PARALLELISM
        );
    }

    #[tokio::test]
    async fn acquire_permit_unblocks_when_limit_increases() {
        let manager = Arc::new(SftpTransferManager::new());
        manager.set_max_concurrent(1);

        let first = manager.acquire_permit().await;
        let blocked_manager = manager.clone();
        let blocked = tokio::spawn(async move { blocked_manager.acquire_permit().await });
        tokio::time::sleep(Duration::from_millis(25)).await;
        assert!(!blocked.is_finished());

        manager.set_max_concurrent(2);
        let second = tokio::time::timeout(Duration::from_millis(300), blocked)
            .await
            .expect("permit waiter should wake after limit increase")
            .expect("permit task should complete");
        drop(first);
        drop(second);
    }

    fn make_background_snapshot(id: &str, node_id: &str) -> BackgroundTransferSnapshot {
        BackgroundTransferSnapshot::new(
            id.to_string(),
            node_id.to_string(),
            "project/".to_string(),
            "/local/project".to_string(),
            "/remote/project".to_string(),
            BackgroundTransferDirection::Upload,
            BackgroundTransferKind::Directory,
            TransferStrategy::DirectoryTar,
            0,
            0,
        )
    }

    #[test]
    fn background_transfer_snapshot_lifecycle_matches_tauri_manager() {
        let manager = SftpTransferManager::new();
        manager.register_background_transfer(make_background_snapshot("tx-1", "node-a"));

        let queued = manager.get_background_transfer("tx-1").unwrap();
        assert_eq!(queued.state, BackgroundTransferState::Pending);

        manager.mark_background_transfer_active("tx-1");
        manager.update_background_transfer_progress("tx-1", 256, 1024, 64);

        let active = manager.get_background_transfer("tx-1").unwrap();
        assert_eq!(active.state, BackgroundTransferState::Active);
        assert_eq!(active.transferred, 256);
        assert_eq!(active.size, 1024);
        assert_eq!(active.backend_speed, Some(64));
        assert_eq!(manager.list_background_transfers(Some("node-a")).len(), 1);
        assert!(manager.list_background_transfers(Some("node-b")).is_empty());

        let finished = manager
            .finish_background_transfer("tx-1", BackgroundTransferState::Completed, None, Some(7))
            .unwrap();
        assert_eq!(finished.state, BackgroundTransferState::Completed);
        assert_eq!(finished.transferred, 1024);
        assert_eq!(finished.item_count, Some(7));
    }

    #[tokio::test]
    async fn wait_background_transfer_finished_wakes_like_tauri_manager() {
        let manager = Arc::new(SftpTransferManager::new());
        manager.register_background_transfer(make_background_snapshot("tx-1", "node-a"));

        let finisher = manager.clone();
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(25)).await;
            finisher.finish_background_transfer(
                "tx-1",
                BackgroundTransferState::Error,
                Some("boom".to_string()),
                None,
            );
        });

        let snapshot = tokio::time::timeout(
            Duration::from_millis(300),
            manager.wait_background_transfer_finished("tx-1"),
        )
        .await
        .expect("waiter should wake")
        .expect("snapshot should still be retained");

        assert_eq!(snapshot.state, BackgroundTransferState::Error);
        assert_eq!(snapshot.error.as_deref(), Some("boom"));
    }
}
