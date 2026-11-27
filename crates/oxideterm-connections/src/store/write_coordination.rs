use std::{
    cell::Cell,
    collections::HashMap,
    path::{Path, PathBuf},
    sync::{
        Arc, LazyLock, Weak,
        atomic::{AtomicU64, Ordering},
    },
};

use parking_lot::{Mutex, ReentrantMutex};
use sha2::{Digest, Sha256};

use super::{ConnectionStore, Result};

#[derive(Debug)]
pub(super) struct Gate {
    writer: ReentrantMutex<FileRevision>,
}

#[derive(Debug, Default)]
pub(super) struct FileRevision {
    generation: Cell<u64>,
    digest: Cell<Option<[u8; 32]>>,
}

impl FileRevision {
    pub(super) fn get(&self) -> u64 {
        self.generation.get()
    }

    /// Filesystem notifications for the same persisted bytes share one generation.
    pub(super) fn observe_file(&self, bytes: Option<&[u8]>) {
        let digest = bytes.map(|bytes| Sha256::digest(bytes).into());
        if self.digest.replace(digest) != digest {
            self.generation.set(self.get() + 1);
        }
    }

    fn invalidate(&self) {
        self.digest.set(None);
        self.generation.set(self.get() + 1);
    }
}

impl Gate {
    pub(super) fn lock(&self) -> parking_lot::ReentrantMutexGuard<'_, FileRevision> {
        self.writer.lock()
    }
}
static STORES: LazyLock<Mutex<HashMap<PathBuf, Weak<Gate>>>> = LazyLock::new(Default::default);

#[derive(Debug)]
pub(super) struct StoreRevision {
    pub(super) gate: Arc<Gate>,
    generation: AtomicU64,
}

impl Clone for StoreRevision {
    fn clone(&self) -> Self {
        Self {
            gate: self.gate.clone(),
            generation: self.observed().into(),
        }
    }
}

impl StoreRevision {
    pub(super) fn open(path: &Path) -> Self {
        let key = std::fs::canonicalize(path).unwrap_or_else(|_| {
            path.parent()
                .and_then(|dir| dir.canonicalize().ok())
                .and_then(|dir| path.file_name().map(|name| dir.join(name)))
                .unwrap_or_else(|| path.to_path_buf())
        });
        let mut stores = STORES.lock();
        stores.retain(|_, gate| gate.strong_count() != 0);
        let gate = stores.get(&key).and_then(Weak::upgrade).unwrap_or_else(|| {
            let gate = Arc::new(Gate {
                writer: Default::default(),
            });
            stores.insert(key, Arc::downgrade(&gate));
            gate
        });
        Self {
            gate,
            generation: 0.into(),
        }
    }

    pub(super) fn observed(&self) -> u64 {
        self.generation.load(Ordering::Relaxed)
    }

    pub(super) fn observe(&self, generation: u64) {
        self.generation.store(generation, Ordering::Relaxed);
    }
}

impl ConnectionStore {
    pub fn refresh_if_changed(&mut self) -> Result<()> {
        let gate = self.revision.gate.clone();
        let generation = gate.lock();
        self.refresh_for_write(generation.get())
    }

    pub fn reload_after_external_change(&self) -> Result<Self> {
        let gate = &self.revision.gate;
        let generation = gate.lock();
        let loaded = Self::load(self.path());
        let current = loaded
            .as_ref()
            .ok()
            .filter(|s| Arc::ptr_eq(gate, &s.revision.gate));
        if current.is_none() {
            // An unreadable or replaced store can no longer vouch for the previous identity.
            generation.invalidate();
        }
        loaded
    }
    // Readers live in independent windows. Reload while holding the file's writer
    // gate before deciding ownership, conflicts, or which fields to replace.
    pub(super) fn refresh_for_write(&mut self, generation: u64) -> Result<()> {
        if self.revision.observed() != generation {
            let current = Self::load_without_side_effects(self.path.clone())?;
            self.data = current.data;
            self.storage_format = current.storage_format;
            self.revision.observe(current.revision.observed());
        }
        Ok(())
    }
}
