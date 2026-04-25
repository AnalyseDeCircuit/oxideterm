// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

//! Transfer Manager for SFTP operations
//!
//! Provides concurrent transfer control with pause/cancel support.

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use parking_lot::RwLock;
use tokio::sync::{Notify, Semaphore, watch};
use tracing::{debug, info, warn};

/// Transfer control signals
#[derive(Debug)]
pub struct TransferControl {
    /// Cancellation signal via watch channel
    cancel_tx: watch::Sender<bool>,
    cancel_rx: watch::Receiver<bool>,
    /// Pause signal via watch channel (independent from cancellation)
    pause_tx: watch::Sender<bool>,
    pause_rx: watch::Receiver<bool>,
}

impl TransferControl {
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

    /// Get a receiver for waiting on cancellation
    pub fn subscribe_cancellation(&self) -> watch::Receiver<bool> {
        self.cancel_rx.clone()
    }

    /// Get a receiver for waiting on pause state changes
    pub fn subscribe_pause(&self) -> watch::Receiver<bool> {
        self.pause_rx.clone()
    }

    pub fn pause(&self) {
        let _ = self.pause_tx.send(true);
    }

    pub fn resume(&self) {
        let _ = self.pause_tx.send(false);
    }
}

impl Default for TransferControl {
    fn default() -> Self {
        Self::new()
    }
}

/// RAII permit that decrements [`TransferManager::active_count`] on drop.
///
/// Wraps the underlying `OwnedSemaphorePermit` so the semaphore slot is also
/// released automatically.
pub struct TransferPermit {
    _permit: tokio::sync::OwnedSemaphorePermit,
    active_count: Arc<AtomicUsize>,
    availability_notify: Arc<Notify>,
}

impl Drop for TransferPermit {
    fn drop(&mut self) {
        let result = self
            .active_count
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |n| n.checked_sub(1));
        match result {
            Ok(prev) => debug!("TransferPermit dropped, active count: {}", prev - 1),
            Err(_) => warn!("TransferPermit dropped with active_count already 0"),
        }
        self.availability_notify.notify_one();
    }
}

/// RAII guard that automatically unregisters a transfer from [`TransferManager`] on drop.
///
/// This prevents `controls` HashMap entry leaks on **any** early-return path
/// (e.g. `?` operator, explicit `return Err(...)`, panics). Create one
/// immediately after `tm.register()` and let the guard live for the duration
/// of the transfer function.
pub struct TransferGuard {
    manager: Option<Arc<TransferManager>>,
    transfer_id: String,
}

impl TransferGuard {
    /// Wrap an optional `TransferManager` reference.  If `manager` is `None`
    /// the guard becomes a no-op (no-manager scenario).
    pub fn new(manager: Option<&Arc<TransferManager>>, transfer_id: String) -> Self {
        Self {
            manager: manager.cloned(),
            transfer_id,
        }
    }
}

impl Drop for TransferGuard {
    fn drop(&mut self) {
        if let Some(tm) = &self.manager {
            tm.unregister(&self.transfer_id);
        }
    }
}

/// Maximum possible concurrent transfers (semaphore upper bound)
const MAX_POSSIBLE_CONCURRENT: usize = 10;

/// Default concurrent transfers
const DEFAULT_CONCURRENT_TRANSFERS: usize = 3;

/// Maximum recursive directory file workers per transfer.
const MAX_DIRECTORY_PARALLELISM: usize = 16;

/// Default recursive directory file workers per transfer.
const DEFAULT_DIRECTORY_PARALLELISM: usize = 4;

/// Transfer Manager handles concurrent transfers
pub struct TransferManager {
    /// Semaphore for limiting concurrent transfers (sized for max possible)
    semaphore: Arc<Semaphore>,
    /// Active transfer controls
    controls: RwLock<HashMap<String, Arc<TransferControl>>>,
    /// Active transfer count (Arc so TransferPermit can decrement on drop)
    active_count: Arc<AtomicUsize>,
    /// Current configured max concurrent (can be changed at runtime)
    max_concurrent: AtomicUsize,
    /// File worker count used inside recursive directory transfers.
    directory_parallelism: AtomicUsize,
    /// Wake blocked acquirers when capacity changes.
    availability_notify: Arc<Notify>,
    /// Speed limit in bytes per second (0 = unlimited, Arc for sharing with transfer loops)
    speed_limit_bps: Arc<AtomicUsize>,
}

impl TransferManager {
    pub fn new() -> Self {
        Self {
            semaphore: Arc::new(Semaphore::new(MAX_POSSIBLE_CONCURRENT)),
            controls: RwLock::new(HashMap::new()),
            active_count: Arc::new(AtomicUsize::new(0)),
            max_concurrent: AtomicUsize::new(DEFAULT_CONCURRENT_TRANSFERS),
            directory_parallelism: AtomicUsize::new(DEFAULT_DIRECTORY_PARALLELISM),
            availability_notify: Arc::new(Notify::new()),
            speed_limit_bps: Arc::new(AtomicUsize::new(0)),
        }
    }

    /// Update the maximum concurrent transfer limit
    pub fn set_max_concurrent(&self, max: usize) {
        let clamped = max.clamp(1, MAX_POSSIBLE_CONCURRENT);
        self.max_concurrent.store(clamped, Ordering::Release);
        self.availability_notify.notify_waiters();
        info!("Max concurrent transfers set to: {}", clamped);
    }

    /// Update recursive directory file worker count.
    pub fn set_directory_parallelism(&self, parallelism: usize) {
        let clamped = parallelism.clamp(1, MAX_DIRECTORY_PARALLELISM);
        self.directory_parallelism.store(clamped, Ordering::Release);
        info!("SFTP directory transfer parallelism set to: {}", clamped);
    }

    /// Get recursive directory file worker count.
    pub fn directory_parallelism(&self) -> usize {
        self.directory_parallelism.load(Ordering::Acquire)
    }

    /// Update the speed limit (in KB/s, 0 = unlimited)
    pub fn set_speed_limit_kbps(&self, kbps: usize) {
        let bps = kbps * 1024;
        self.speed_limit_bps.store(bps, Ordering::Release);
        if kbps > 0 {
            info!("Speed limit set to: {} KB/s", kbps);
        } else {
            info!("Speed limit disabled (unlimited)");
        }
    }

    /// Get current speed limit in bytes per second (0 = unlimited)
    pub fn get_speed_limit_bps(&self) -> usize {
        self.speed_limit_bps.load(Ordering::Acquire)
    }

    /// Get a shared reference to the speed limit atomic for passing to transfer loops
    pub fn speed_limit_bps_ref(&self) -> Arc<AtomicUsize> {
        self.speed_limit_bps.clone()
    }

    /// Register a new transfer and get its control handle
    pub fn register(&self, transfer_id: &str) -> Arc<TransferControl> {
        let control = Arc::new(TransferControl::new());
        self.controls
            .write()
            .insert(transfer_id.to_string(), control.clone());
        info!("Registered transfer: {}", transfer_id);
        control
    }

    /// Get control handle for a transfer
    pub fn get_control(&self, transfer_id: &str) -> Option<Arc<TransferControl>> {
        self.controls.read().get(transfer_id).cloned()
    }

    /// Remove a transfer from tracking
    pub fn unregister(&self, transfer_id: &str) {
        self.controls.write().remove(transfer_id);
        debug!("Unregistered transfer: {}", transfer_id);
    }

    /// Acquire a permit for concurrent transfer (blocks if at limit)
    ///
    /// Returns a [`TransferPermit`] that automatically decrements `active_count`
    /// and releases the semaphore slot when dropped.
    ///
    /// Uses a soft limit approach: the semaphore has MAX_POSSIBLE_CONCURRENT permits,
    /// but we wait until active_count < max_concurrent before acquiring.
    ///
    /// # Panics
    /// This function will panic if the semaphore is closed, which should never happen
    /// in normal operation as the semaphore lives for the lifetime of the TransferManager.
    pub async fn acquire_permit(&self) -> TransferPermit {
        loop {
            let notified = self.availability_notify.notified();
            let current = self.active_count.load(Ordering::Acquire);
            let max = self.max_concurrent.load(Ordering::Acquire);
            if current < max {
                break;
            }
            notified.await;
        }

        let permit = self
            .semaphore
            .clone()
            .acquire_owned()
            .await
            .unwrap_or_else(|_| {
                // This should never happen as we own the semaphore and never close it
                panic!("TransferManager semaphore was unexpectedly closed - this is a bug")
            });
        let new_count = self.active_count.fetch_add(1, Ordering::AcqRel) + 1;
        debug!(
            "Acquired transfer permit, active count: {}/{}",
            new_count,
            self.max_concurrent.load(Ordering::Relaxed)
        );
        TransferPermit {
            _permit: permit,
            active_count: self.active_count.clone(),
            availability_notify: self.availability_notify.clone(),
        }
    }

    /// Get current active transfer count
    pub fn active_count(&self) -> usize {
        self.active_count.load(Ordering::Acquire)
    }

    /// Get maximum concurrent transfers
    pub fn max_concurrent(&self) -> usize {
        self.max_concurrent.load(Ordering::Acquire)
    }

    /// Get the number of currently registered (tracked) transfers
    pub fn registered_count(&self) -> usize {
        self.controls.read().len()
    }

    /// Cancel a specific transfer
    pub fn cancel(&self, transfer_id: &str) -> bool {
        if let Some(control) = self.controls.read().get(transfer_id) {
            control.cancel();
            info!("Cancelled transfer: {}", transfer_id);
            true
        } else {
            warn!("Transfer not found for cancel: {}", transfer_id);
            false
        }
    }

    /// Pause a specific transfer (keeps temp files, can be resumed)
    pub fn pause(&self, transfer_id: &str) -> bool {
        if let Some(control) = self.controls.read().get(transfer_id) {
            control.pause();
            info!("Paused transfer: {}", transfer_id);
            true
        } else {
            warn!("Transfer not found for pause: {}", transfer_id);
            false
        }
    }

    /// Resume a paused transfer
    pub fn resume(&self, transfer_id: &str) -> bool {
        if let Some(control) = self.controls.read().get(transfer_id) {
            control.resume();
            info!("Resumed transfer: {}", transfer_id);
            true
        } else {
            warn!("Transfer not found for resume: {}", transfer_id);
            false
        }
    }

    /// Cancel all active transfers
    pub fn cancel_all(&self) {
        let controls = self.controls.read();
        for (id, control) in controls.iter() {
            control.cancel();
            info!("Cancelled transfer: {}", id);
        }
    }

    /// Decrement active count (called when transfer completes)
    pub fn on_transfer_complete(&self) {
        let result = self
            .active_count
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |n| n.checked_sub(1));
        match result {
            Ok(prev) => debug!("Transfer complete, active count: {}", prev - 1),
            Err(_) => warn!("on_transfer_complete called with active_count already 0"),
        }
        self.availability_notify.notify_one();
    }
}

impl Default for TransferManager {
    fn default() -> Self {
        Self::new()
    }
}

/// Check loop helper for pause/cancel during transfer
pub async fn check_transfer_control(
    control: &TransferControl,
) -> Result<(), super::error::SftpError> {
    // Simplified: only check cancellation (pause = cancel in v0.1.0)
    if control.is_cancelled() {
        return Err(super::error::SftpError::TransferCancelled);
    }

    // Note: Pause functionality removed - pause now directly cancels the transfer
    // Users must restart the transfer to continue

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sftp::SftpError;
    use std::sync::Arc;
    use std::time::Duration;

    use tokio::time::timeout;

    #[test]
    fn test_transfer_control_state_transitions() {
        let control = TransferControl::new();
        assert!(!control.is_cancelled());
        assert!(!control.is_paused());

        control.pause();
        assert!(control.is_paused());

        control.resume();
        assert!(!control.is_paused());

        control.cancel();
        assert!(control.is_cancelled());
    }

    #[test]
    fn test_transfer_control_subscribers_observe_changes() {
        let control = TransferControl::new();
        let cancel_rx = control.subscribe_cancellation();
        let pause_rx = control.subscribe_pause();

        control.pause();
        control.cancel();

        assert!(*pause_rx.borrow());
        assert!(*cancel_rx.borrow());
    }

    #[test]
    fn test_register_same_transfer_id_replaces_existing_control() {
        let manager = TransferManager::new();
        let first = manager.register("tx-1");
        let second = manager.register("tx-1");

        assert_eq!(manager.registered_count(), 1);
        assert!(!Arc::ptr_eq(&first, &second));
        assert!(Arc::ptr_eq(&manager.get_control("tx-1").unwrap(), &second));
    }

    #[test]
    fn test_transfer_guard_unregisters_on_drop() {
        let manager = Arc::new(TransferManager::new());
        manager.register("guarded-transfer");
        assert_eq!(manager.registered_count(), 1);

        {
            let _guard = TransferGuard::new(Some(&manager), "guarded-transfer".to_string());
            assert_eq!(manager.registered_count(), 1);
        }

        assert_eq!(manager.registered_count(), 0);
    }

    #[test]
    fn test_transfer_guard_noop_without_manager() {
        let _guard = TransferGuard::new(None, "no-manager".to_string());
    }

    #[test]
    fn test_set_max_concurrent_is_clamped() {
        let manager = TransferManager::new();

        manager.set_max_concurrent(0);
        assert_eq!(manager.max_concurrent(), 1);

        manager.set_max_concurrent(MAX_POSSIBLE_CONCURRENT + 10);
        assert_eq!(manager.max_concurrent(), MAX_POSSIBLE_CONCURRENT);
    }

    #[test]
    fn test_set_speed_limit_kbps() {
        let manager = TransferManager::new();

        manager.set_speed_limit_kbps(256);
        assert_eq!(manager.get_speed_limit_bps(), 256 * 1024);

        manager.set_speed_limit_kbps(0);
        assert_eq!(manager.get_speed_limit_bps(), 0);
    }

    #[test]
    fn test_set_directory_parallelism_is_clamped() {
        let manager = TransferManager::new();
        assert_eq!(
            manager.directory_parallelism(),
            DEFAULT_DIRECTORY_PARALLELISM
        );

        manager.set_directory_parallelism(0);
        assert_eq!(manager.directory_parallelism(), 1);

        manager.set_directory_parallelism(MAX_DIRECTORY_PARALLELISM + 10);
        assert_eq!(manager.directory_parallelism(), MAX_DIRECTORY_PARALLELISM);
    }

    #[test]
    fn test_cancel_pause_resume_missing_transfer() {
        let manager = TransferManager::new();

        assert!(!manager.cancel("missing"));
        assert!(!manager.pause("missing"));
        assert!(!manager.resume("missing"));
    }

    #[test]
    fn test_cancel_all_marks_everything_cancelled() {
        let manager = TransferManager::new();
        let first = manager.register("tx-1");
        let second = manager.register("tx-2");

        manager.cancel_all();

        assert!(first.is_cancelled());
        assert!(second.is_cancelled());
    }

    #[test]
    fn test_on_transfer_complete_underflow_is_safe() {
        let manager = TransferManager::new();
        manager.on_transfer_complete();
        assert_eq!(manager.active_count(), 0);
    }

    #[tokio::test]
    async fn test_acquire_permit_blocks_until_previous_is_dropped() {
        let manager = Arc::new(TransferManager::new());
        manager.set_max_concurrent(1);

        let permit = manager.acquire_permit().await;
        assert_eq!(manager.active_count(), 1);

        let blocked = timeout(Duration::from_millis(50), manager.acquire_permit()).await;
        assert!(blocked.is_err());

        drop(permit);
        assert_eq!(manager.active_count(), 0);

        let second = timeout(Duration::from_millis(300), manager.acquire_permit())
            .await
            .expect("second permit should be acquired after the first is dropped");
        assert_eq!(manager.active_count(), 1);
        drop(second);
        assert_eq!(manager.active_count(), 0);
    }

    #[tokio::test]
    async fn test_acquire_permit_unblocks_when_limit_increases() {
        let manager = Arc::new(TransferManager::new());
        manager.set_max_concurrent(1);

        let first = manager.acquire_permit().await;
        let blocked_manager = manager.clone();
        let blocked = tokio::spawn(async move { blocked_manager.acquire_permit().await });

        tokio::time::sleep(Duration::from_millis(25)).await;
        manager.set_max_concurrent(2);

        let second = timeout(Duration::from_millis(150), blocked)
            .await
            .expect("permit waiter should wake after limit increase")
            .expect("task should complete");

        assert_eq!(manager.active_count(), 2);

        drop(first);
        drop(second);
        assert_eq!(manager.active_count(), 0);
    }

    #[tokio::test]
    async fn test_check_transfer_control_only_cares_about_cancel() {
        let control = TransferControl::new();
        control.pause();
        assert!(check_transfer_control(&control).await.is_ok());

        control.cancel();
        let result = check_transfer_control(&control).await;
        assert!(matches!(result, Err(SftpError::TransferCancelled)));
    }
}
