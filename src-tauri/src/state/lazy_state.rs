// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use std::path::PathBuf;
use std::sync::Arc;

use super::lazy_store::LazyManagedStore;
use super::store::{StateError, StateStore};

pub struct LazyStateStore {
    store: LazyManagedStore<StateStore>,
}

impl LazyStateStore {
    pub fn new(path: PathBuf) -> Self {
        let init_path = path.clone();

        Self {
            store: LazyManagedStore::new("state store", move || {
                StateStore::new(init_path.clone())
                    .map_err(|error| format!("Failed to initialize state store at {:?}: {}", init_path, error))
            }),
        }
    }

    fn resolve_store(&self) -> Result<Arc<StateStore>, StateError> {
        self.store
            .resolve()
            .map_err(StateError::Initialization)
    }

    pub fn save_session(&self, id: &str, data: &[u8]) -> Result<(), StateError> {
        self.resolve_store()?.save_session(id, data)
    }

    pub async fn save_session_async(&self, id: String, data: Vec<u8>) -> Result<(), StateError> {
        self.resolve_store()?.save_session_async(id, data).await
    }

    pub fn load_session(&self, id: &str) -> Result<Vec<u8>, StateError> {
        self.resolve_store()?.load_session(id)
    }

    pub fn delete_session(&self, id: &str) -> Result<(), StateError> {
        self.resolve_store()?.delete_session(id)
    }

    pub async fn delete_session_async(&self, id: String) -> Result<(), StateError> {
        self.resolve_store()?.delete_session_async(id).await
    }

    pub fn list_sessions(&self) -> Result<Vec<String>, StateError> {
        self.resolve_store()?.list_sessions()
    }

    pub async fn load_all_sessions_async(&self) -> Result<Vec<(String, Vec<u8>)>, StateError> {
        self.resolve_store()?.load_all_sessions_async().await
    }

    pub fn save_forward(&self, id: &str, data: &[u8]) -> Result<(), StateError> {
        self.resolve_store()?.save_forward(id, data)
    }

    pub async fn save_forward_async(&self, id: String, data: Vec<u8>) -> Result<(), StateError> {
        self.resolve_store()?.save_forward_async(id, data).await
    }

    pub fn load_forward(&self, id: &str) -> Result<Vec<u8>, StateError> {
        self.resolve_store()?.load_forward(id)
    }

    pub fn list_forwards(&self) -> Result<Vec<String>, StateError> {
        self.resolve_store()?.list_forwards()
    }

    pub fn delete_forward(&self, id: &str) -> Result<(), StateError> {
        self.resolve_store()?.delete_forward(id)
    }

    pub async fn delete_forward_async(&self, id: String) -> Result<(), StateError> {
        self.resolve_store()?.delete_forward_async(id).await
    }

    pub async fn load_all_forwards_async(&self) -> Result<Vec<(String, Vec<u8>)>, StateError> {
        self.resolve_store()?.load_all_forwards_async().await
    }

    pub fn save_forward_tombstone(&self, id: &str, data: &[u8]) -> Result<(), StateError> {
        self.resolve_store()?.save_forward_tombstone(id, data)
    }

    pub async fn save_forward_tombstone_async(
        &self,
        id: String,
        data: Vec<u8>,
    ) -> Result<(), StateError> {
        self.resolve_store()?.save_forward_tombstone_async(id, data).await
    }

    pub fn load_all_forward_tombstones(&self) -> Result<Vec<(String, Vec<u8>)>, StateError> {
        self.resolve_store()?.load_all_forward_tombstones()
    }

    pub async fn load_all_forward_tombstones_async(
        &self,
    ) -> Result<Vec<(String, Vec<u8>)>, StateError> {
        self.resolve_store()?.load_all_forward_tombstones_async().await
    }

    pub fn delete_forward_tombstone(&self, id: &str) -> Result<(), StateError> {
        self.resolve_store()?.delete_forward_tombstone(id)
    }

    pub async fn delete_forward_tombstone_async(&self, id: String) -> Result<(), StateError> {
        self.resolve_store()?.delete_forward_tombstone_async(id).await
    }

    pub async fn load_all_forward_sync_state_async(
        &self,
    ) -> Result<(Vec<(String, Vec<u8>)>, Vec<(String, Vec<u8>)>), StateError> {
        self.resolve_store()?.load_all_forward_sync_state_async().await
    }

    pub async fn replace_forward_with_tombstone_async(
        &self,
        id: String,
        tombstone_data: Vec<u8>,
    ) -> Result<bool, StateError> {
        self.resolve_store()?
            .replace_forward_with_tombstone_async(id, tombstone_data)
            .await
    }
}

#[cfg(test)]
mod tests {
    use super::LazyStateStore;
    use crate::state::{StateError, StateStore};
    use std::time::{Duration, Instant};
    use tempfile::TempDir;

    #[derive(Debug, Clone)]
    struct TimingStats {
        min: Duration,
        median: Duration,
        p95: Duration,
        max: Duration,
        average: Duration,
    }

    impl TimingStats {
        fn from_samples(mut samples: Vec<Duration>) -> Self {
            assert!(!samples.is_empty());
            samples.sort_unstable();

            let len = samples.len();
            let median = samples[len / 2];
            let p95_index = ((len - 1) * 95) / 100;
            let total_nanos: u128 = samples.iter().map(|sample| sample.as_nanos()).sum();
            let average = Duration::from_nanos((total_nanos / len as u128) as u64);

            Self {
                min: samples[0],
                median,
                p95: samples[p95_index],
                max: samples[len - 1],
                average,
            }
        }

        fn format_ms(duration: Duration) -> String {
            format!("{:.3} ms", duration.as_secs_f64() * 1000.0)
        }

        fn log(&self, label: &str) {
            eprintln!(
                "{label}: avg={}, median={}, p95={}, min={}, max={}",
                Self::format_ms(self.average),
                Self::format_ms(self.median),
                Self::format_ms(self.p95),
                Self::format_ms(self.min),
                Self::format_ms(self.max)
            );
        }
    }

    fn measure_durations(iterations: usize, mut run: impl FnMut() -> Duration) -> TimingStats {
        let mut samples = Vec::with_capacity(iterations);
        for _ in 0..iterations {
            samples.push(run());
        }
        TimingStats::from_samples(samples)
    }

    #[test]
    fn does_not_create_database_before_first_use() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("lazy-state.redb");
        let store = LazyStateStore::new(db_path.clone());

        assert!(std::fs::symlink_metadata(&db_path).is_err());

        let sessions = store.list_sessions().unwrap();

        assert!(sessions.is_empty());
        assert!(std::fs::symlink_metadata(&db_path).is_ok());
    }

    #[test]
    fn initialization_failure_is_reported_as_state_error() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("missing").join("lazy-state.redb");
        let store = LazyStateStore::new(db_path);

        let error = store.list_sessions().unwrap_err();
        assert!(matches!(error, StateError::Initialization(_)));
    }

    #[test]
    #[ignore = "manual measurement"]
    fn measure_eager_vs_lazy_state_store_latency() {
        const ITERATIONS: usize = 60;

        let eager_startup = measure_durations(ITERATIONS, || {
            let temp_dir = TempDir::new().unwrap();
            let db_path = temp_dir.path().join("measure.redb");
            let start = Instant::now();
            let _store = StateStore::new(db_path).unwrap();
            start.elapsed()
        });

        let lazy_registration = measure_durations(ITERATIONS, || {
            let temp_dir = TempDir::new().unwrap();
            let db_path = temp_dir.path().join("measure.redb");
            let start = Instant::now();
            let _store = LazyStateStore::new(db_path);
            start.elapsed()
        });

        let eager_first_persistence = measure_durations(ITERATIONS, || {
            let temp_dir = TempDir::new().unwrap();
            let db_path = temp_dir.path().join("measure.redb");
            let store = StateStore::new(db_path).unwrap();
            let start = Instant::now();
            let sessions = store.list_sessions().unwrap();
            assert!(sessions.is_empty());
            start.elapsed()
        });

        let lazy_first_touch = measure_durations(ITERATIONS, || {
            let temp_dir = TempDir::new().unwrap();
            let db_path = temp_dir.path().join("measure.redb");
            let store = LazyStateStore::new(db_path);
            let start = Instant::now();
            let sessions = store.list_sessions().unwrap();
            assert!(sessions.is_empty());
            start.elapsed()
        });

        let lazy_second_touch = measure_durations(ITERATIONS, || {
            let temp_dir = TempDir::new().unwrap();
            let db_path = temp_dir.path().join("measure.redb");
            let store = LazyStateStore::new(db_path);
            assert!(store.list_sessions().unwrap().is_empty());
            let start = Instant::now();
            let sessions = store.list_sessions().unwrap();
            assert!(sessions.is_empty());
            start.elapsed()
        });

        eager_startup.log("eager startup path (StateStore::new)");
        lazy_registration.log("lazy startup path (LazyStateStore::new)");
        eager_first_persistence.log("eager first persistence after startup");
        lazy_first_touch.log("lazy first persistence touch");
        lazy_second_touch.log("lazy second persistence touch");
    }
}