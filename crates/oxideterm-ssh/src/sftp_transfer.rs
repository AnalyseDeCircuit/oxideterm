// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

use tokio::sync::{Notify, Semaphore};

pub const DEFAULT_SFTP_CONCURRENT_TRANSFERS: usize = 3;
pub const DEFAULT_SFTP_DIRECTORY_PARALLELISM: usize = 4;
pub const MAX_SFTP_CONCURRENT_TRANSFERS: usize = 10;
pub const MAX_SFTP_DIRECTORY_PARALLELISM: usize = 16;

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
pub struct SftpTransferManager {
    semaphore: Arc<Semaphore>,
    active_count: Arc<AtomicUsize>,
    max_concurrent_transfers: AtomicUsize,
    directory_parallelism: AtomicUsize,
    speed_limit_bps: AtomicUsize,
    availability_notify: Arc<Notify>,
}

impl SftpTransferManager {
    pub fn new() -> Self {
        Self {
            semaphore: Arc::new(Semaphore::new(MAX_SFTP_CONCURRENT_TRANSFERS)),
            active_count: Arc::new(AtomicUsize::new(0)),
            max_concurrent_transfers: AtomicUsize::new(DEFAULT_SFTP_CONCURRENT_TRANSFERS),
            directory_parallelism: AtomicUsize::new(DEFAULT_SFTP_DIRECTORY_PARALLELISM),
            speed_limit_bps: AtomicUsize::new(0),
            availability_notify: Arc::new(Notify::new()),
        }
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
}
