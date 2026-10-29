// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use crate::rag::embedding::VectorHit;
use crate::rag::error::RagError;
use crate::rag::types::EmbeddingRecord;
use instant_distance::{Builder, HnswMap, Point, Search};
use oxideterm_atomic_file::durable_write_stream;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::io::Read;
use std::path::Path;
use tracing::{debug, info, warn};

pub enum HnswLoadOutcome {
    Loaded(PersistedHnswIndex),
    Missing,
    Failed(String),
}

// ═══════════════════════════════════════════════════════════════════════════
// HNSW Builder Parameters
// ═══════════════════════════════════════════════════════════════════════════

const EF_CONSTRUCTION: usize = 200;
const EF_SEARCH: usize = 100;
/// Over-fetch factor: request more candidates than top_k to compensate
/// for post-hoc collection filtering.
const OVERFETCH_FACTOR: usize = 3;

// ═══════════════════════════════════════════════════════════════════════════
// CosinePoint — wrapper implementing instant_distance::Point
// ═══════════════════════════════════════════════════════════════════════════

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CosinePoint {
    vector: Vec<f32>,
    norm: f32,
}

impl CosinePoint {
    pub fn new(vector: Vec<f32>) -> Self {
        let norm = l2_norm(&vector);
        Self { vector, norm }
    }
}

impl Point for CosinePoint {
    fn distance(&self, other: &Self) -> f32 {
        if self.norm == 0.0 || other.norm == 0.0 {
            return 1.0; // maximum distance for zero vectors
        }
        let dot: f32 = self
            .vector
            .iter()
            .zip(other.vector.iter())
            .map(|(a, b)| a * b)
            .sum();
        let cosine_sim = dot / (self.norm * other.norm);
        // Clamp to [0, 2] range — distance is 1 - similarity
        (1.0 - cosine_sim).clamp(0.0, 2.0)
    }
}

fn l2_norm(v: &[f32]) -> f32 {
    v.iter().map(|x| x * x).sum::<f32>().sqrt()
}

// ═══════════════════════════════════════════════════════════════════════════
// Index Metadata
// ═══════════════════════════════════════════════════════════════════════════

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HnswIndexMeta {
    pub dimensions: usize,
    pub model_name: String,
    pub point_count: usize,
    pub built_at: i64,
}

// ═══════════════════════════════════════════════════════════════════════════
// PersistedHnswIndex — the main HNSW wrapper
// ═══════════════════════════════════════════════════════════════════════════

#[derive(Serialize, Deserialize)]
pub struct PersistedHnswIndex {
    pub meta: HnswIndexMeta,
    map: HnswMap<CosinePoint, String>,
}

impl PersistedHnswIndex {
    /// Build an HNSW index from a set of embedding records.
    /// Returns `None` if the input is empty.
    pub fn build(embeddings: &[EmbeddingRecord]) -> Option<Self> {
        Self::build_owned(embeddings.to_vec())
    }

    pub(crate) fn build_owned(embeddings: Vec<EmbeddingRecord>) -> Option<Self> {
        if embeddings.is_empty() {
            return None;
        }

        let first = &embeddings[0];
        let dimensions = first.dimensions;
        let model_name = first.model_name.clone();

        let (points, values): (Vec<CosinePoint>, Vec<String>) = embeddings
            .into_iter()
            .filter(|e| e.dimensions == dimensions)
            .map(|e| (CosinePoint::new(e.vector), e.chunk_id))
            .unzip();

        if points.is_empty() {
            return None;
        }

        let point_count = points.len();
        info!(
            "Building HNSW index: {} points, {} dimensions, model={}",
            point_count, dimensions, model_name
        );

        let map = Builder::default()
            .ef_construction(EF_CONSTRUCTION)
            .ef_search(EF_SEARCH)
            .build(points, values);

        let meta = HnswIndexMeta {
            dimensions,
            model_name,
            point_count,
            built_at: chrono::Utc::now().timestamp_millis(),
        };

        info!("HNSW index built: {} points indexed", point_count);
        Some(Self { meta, map })
    }

    /// Search the HNSW index, optionally filtering by a set of allowed chunk IDs.
    ///
    /// If `allowed_chunk_ids` is `Some`, only results within that set are returned.
    /// Uses over-fetching to compensate for filtered-out results.
    pub fn search(
        &self,
        query_vector: &[f32],
        top_k: usize,
        allowed_chunk_ids: Option<&HashSet<String>>,
    ) -> Vec<VectorHit> {
        if query_vector.len() != self.meta.dimensions {
            return Vec::new();
        }

        let query_point = CosinePoint::new(query_vector.to_vec());
        let mut search = Search::default();

        let fetch_count = match allowed_chunk_ids {
            Some(_) => top_k * OVERFETCH_FACTOR,
            None => top_k,
        };

        let results: Vec<VectorHit> = self
            .map
            .search(&query_point, &mut search)
            .take(fetch_count)
            .filter_map(|item| {
                let chunk_id = item.value;
                // Filter by allowed set if provided
                if let Some(allowed) = allowed_chunk_ids {
                    if !allowed.contains(chunk_id) {
                        return None;
                    }
                }
                // Convert distance back to similarity score
                let score = (1.0 - item.distance) as f64;
                Some(VectorHit {
                    chunk_id: chunk_id.clone(),
                    score,
                })
            })
            .take(top_k)
            .collect();

        results
    }

    /// Serialize the index to a file (rmp-serde + zstd compression).
    pub fn save(&self, path: &Path) -> Result<(), RagError> {
        durable_write_stream(path, |file| {
            let encoder = zstd::stream::write::Encoder::new(file, 3)?;
            let mut buffered = std::io::BufWriter::with_capacity(64 * 1024, encoder);
            rmp_serde::encode::write(&mut buffered, self).map_err(std::io::Error::other)?;
            let encoder = buffered.into_inner().map_err(|error| error.into_error())?;
            encoder.finish()?;
            Ok(())
        })?;

        // Set restrictive permissions on Unix
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600));
        }

        info!(
            "HNSW index saved: {} points, {:.1} KB compressed",
            self.meta.point_count,
            path.metadata()?.len() as f64 / 1024.0
        );
        Ok(())
    }

    /// Load a persisted index from file.
    /// Returns `None` if the file doesn't exist or is corrupt.
    pub fn load(path: &Path) -> Option<Self> {
        match Self::load_detailed(path) {
            HnswLoadOutcome::Loaded(index) => Some(index),
            HnswLoadOutcome::Missing | HnswLoadOutcome::Failed(_) => None,
        }
    }

    /// Load a persisted index from file with detailed outcome for lazy state machines.
    pub fn load_detailed(path: &Path) -> HnswLoadOutcome {
        /// Maximum allowed compressed file size (512 MB).
        const MAX_INDEX_FILE_SIZE: u64 = 512 * 1024 * 1024;
        /// Maximum allowed decompressed data size (2 GB).
        const MAX_DECOMPRESSED_SIZE: usize = 2 * 1024 * 1024 * 1024;

        if !path.exists() {
            debug!("No HNSW index file at {:?}", path);
            return HnswLoadOutcome::Missing;
        }

        let file = match std::fs::File::open(path) {
            Ok(f) => f,
            Err(e) => {
                warn!("Failed to open HNSW index file: {}", e);
                return HnswLoadOutcome::Failed(format!("open: {e}"));
            }
        };

        // Check file size before reading
        if let Ok(metadata) = file.metadata() {
            if metadata.len() > MAX_INDEX_FILE_SIZE {
                warn!(
                    "HNSW index file too large: {} bytes (max {})",
                    metadata.len(),
                    MAX_INDEX_FILE_SIZE
                );
                return HnswLoadOutcome::Failed(format!(
                    "file too large: {} bytes exceeds {}",
                    metadata.len(),
                    MAX_INDEX_FILE_SIZE
                ));
            }
        }

        let decoder = match zstd::stream::read::Decoder::new(file) {
            Ok(d) => d,
            Err(e) => {
                warn!("Failed to decompress HNSW index: {}", e);
                return HnswLoadOutcome::Failed(format!("decompress: {e}"));
            }
        };

        // Bound decompression while reading, rather than after allocating the whole document.
        let mut data = std::io::BufReader::with_capacity(
            64 * 1024,
            decoder.take(MAX_DECOMPRESSED_SIZE as u64 + 1),
        );
        match rmp_serde::from_read::<_, Self>(&mut data) {
            Ok(index) => {
                if let Err(error) = std::io::copy(&mut data, &mut std::io::sink()) {
                    return HnswLoadOutcome::Failed(format!("decompress: {error}"));
                }
                if data.get_ref().limit() == 0 {
                    return HnswLoadOutcome::Failed("decompressed index exceeds size limit".into());
                }
                info!(
                    "HNSW index loaded: {} points, {} dimensions, model={}",
                    index.meta.point_count, index.meta.dimensions, index.meta.model_name
                );
                HnswLoadOutcome::Loaded(index)
            }
            Err(e) => {
                warn!("Failed to deserialize HNSW index: {}", e);
                HnswLoadOutcome::Failed(format!("deserialize: {e}"))
            }
        }
    }

    /// Check if this index is compatible with the given query dimensions.
    pub fn is_compatible(&self, dimensions: usize) -> bool {
        self.meta.dimensions == dimensions
    }
}

/// Return the canonical path for the HNSW index file.
pub fn hnsw_index_path(data_dir: &Path) -> std::path::PathBuf {
    data_dir.join("rag_hnsw.bin")
}

// ═══════════════════════════════════════════════════════════════════════════
// Tests
// ═══════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[cfg(target_os = "macos")]
    #[ignore = "manual HNSW serialization/load peak and latency comparison"]
    fn index_io_memory() {
        use std::sync::{
            Arc,
            atomic::{AtomicBool, AtomicUsize, Ordering},
        };
        #[repr(C)]
        #[derive(Default)]
        struct Stats {
            blocks: u32,
            live: usize,
            peak: usize,
            allocated: usize,
        }
        unsafe extern "C" {
            fn malloc_zone_statistics(zone: *mut std::ffi::c_void, stats: *mut Stats);
        }
        fn live() -> usize {
            let mut s = Stats::default();
            unsafe { malloc_zone_statistics(std::ptr::null_mut(), &mut s) };
            s.live
        }
        let mut random = 17u32;
        let records = (0..1024)
            .map(|i| {
                make_embedding(
                    &format!("chunk-{i}"),
                    (0..1536)
                        .map(|_| {
                            random = random.wrapping_mul(1664525).wrapping_add(1013904223);
                            (random >> 8) as f32 / 16777216.0
                        })
                        .collect(),
                )
            })
            .collect();
        let index = PersistedHnswIndex::build_owned(records).unwrap();
        let directory = tempfile::tempdir().unwrap();
        for streamed in [false, true] {
            let path = directory.path().join(format!("index-{streamed}"));
            let baseline = live();
            let peak = Arc::new(AtomicUsize::new(baseline));
            let stop = Arc::new(AtomicBool::new(false));
            let sampler = std::thread::spawn({
                let peak = peak.clone();
                let stop = stop.clone();
                move || {
                    while !stop.load(Ordering::Acquire) {
                        peak.fetch_max(live(), Ordering::Relaxed);
                        std::thread::sleep(std::time::Duration::from_millis(1));
                    }
                }
            });
            let start = std::time::Instant::now();
            if streamed {
                index.save(&path).unwrap();
            } else {
                let data = rmp_serde::to_vec(&index).unwrap();
                let compressed = zstd::encode_all(data.as_slice(), 3).unwrap();
                oxideterm_atomic_file::durable_write(&path, &compressed).unwrap();
            }
            let save_ms = start.elapsed().as_secs_f64() * 1000.0;
            let start = std::time::Instant::now();
            let loaded = if streamed {
                PersistedHnswIndex::load(&path).unwrap()
            } else {
                let compressed = std::fs::read(&path).unwrap();
                let data = zstd::decode_all(compressed.as_slice()).unwrap();
                rmp_serde::from_slice::<PersistedHnswIndex>(&data).unwrap()
            };
            let load_ms = start.elapsed().as_secs_f64() * 1000.0;
            assert_eq!(loaded.meta.point_count, 1024);
            peak.fetch_max(live(), Ordering::Relaxed);
            stop.store(true, Ordering::Release);
            sampler.join().unwrap();
            eprintln!(
                "INDEX_IO streamed={streamed} peak_delta={} live_delta={} save_ms={save_ms:.3} load_ms={load_ms:.3}",
                peak.load(Ordering::Relaxed).saturating_sub(baseline),
                live().saturating_sub(baseline)
            );
            std::hint::black_box(loaded);
        }
    }

    fn make_embedding(chunk_id: &str, vector: Vec<f32>) -> EmbeddingRecord {
        let dimensions = vector.len();
        EmbeddingRecord {
            chunk_id: chunk_id.to_string(),
            vector,
            model_name: "test-model".to_string(),
            dimensions,
        }
    }

    #[test]
    fn persisted_index_preserves_ranked_results_and_collection_filters() {
        let embeddings = vec![
            make_embedding("a", vec![1.0, 0.0, 0.0]),
            make_embedding("b", vec![0.0, 1.0, 0.0]),
            make_embedding("c", vec![0.0, 0.0, 1.0]),
            make_embedding("d", vec![0.7, 0.7, 0.0]),
        ];
        let index = PersistedHnswIndex::build(&embeddings).unwrap();
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("index.bin");
        index.save(&path).unwrap();
        let loaded = PersistedHnswIndex::load(&path).unwrap();
        let legacy_path = dir.path().join("legacy.bin");
        std::fs::write(
            &legacy_path,
            zstd::encode_all(rmp_serde::to_vec(&index).unwrap().as_slice(), 3).unwrap(),
        )
        .unwrap();
        let legacy = PersistedHnswIndex::load(&legacy_path).unwrap();
        let allowed = HashSet::from(["c".to_string(), "d".to_string()]);
        for index in [&index, &loaded, &legacy] {
            assert_eq!(index.meta.point_count, 4);
            assert_eq!(index.meta.dimensions, 3);
            let hits = index.search(&[0.9, 0.1, 0.0], 2, None);
            assert_eq!(
                hits.iter()
                    .map(|hit| hit.chunk_id.as_str())
                    .collect::<Vec<_>>(),
                vec!["a", "d"]
            );
            assert!(hits[0].score > 0.9);
            let filtered = index.search(&[0.9, 0.1, 0.0], 2, Some(&allowed));
            assert_eq!(
                filtered
                    .iter()
                    .map(|hit| hit.chunk_id.as_str())
                    .collect::<Vec<_>>(),
                vec!["d", "c"]
            );
        }
    }
}
