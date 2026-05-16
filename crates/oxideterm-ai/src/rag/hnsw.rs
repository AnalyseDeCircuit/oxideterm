// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use crate::rag::embedding::VectorHit;
use crate::rag::error::RagError;
use crate::rag::types::EmbeddingRecord;
use instant_distance::{Builder, HnswMap, Point, Search};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::io::{Read, Write};
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
        if embeddings.is_empty() {
            return None;
        }

        let first = &embeddings[0];
        let dimensions = first.dimensions;
        let model_name = first.model_name.clone();

        let (points, values): (Vec<CosinePoint>, Vec<String>) = embeddings
            .iter()
            .filter(|e| e.dimensions == dimensions)
            .map(|e| (CosinePoint::new(e.vector.clone()), e.chunk_id.clone()))
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
        let data =
            rmp_serde::to_vec(self).map_err(|e| RagError::HnswIndex(format!("serialize: {e}")))?;

        let compressed = zstd::encode_all(data.as_slice(), 3)
            .map_err(|e| RagError::HnswIndex(format!("compress: {e}")))?;

        let tmp_path = path.with_extension("bin.tmp");
        let mut file = std::fs::File::create(&tmp_path)?;
        file.write_all(&compressed)?;
        file.sync_all()?;
        drop(file);

        // Atomic rename
        std::fs::rename(&tmp_path, path)?;

        // Set restrictive permissions on Unix
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600));
        }

        info!(
            "HNSW index saved: {} points, {:.1} KB compressed",
            self.meta.point_count,
            compressed.len() as f64 / 1024.0
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

        let mut file = match std::fs::File::open(path) {
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

        let mut compressed = Vec::new();
        if let Err(e) = file.read_to_end(&mut compressed) {
            warn!("Failed to read HNSW index file: {}", e);
            return HnswLoadOutcome::Failed(format!("read: {e}"));
        }

        let data = match zstd::decode_all(compressed.as_slice()) {
            Ok(d) => d,
            Err(e) => {
                warn!("Failed to decompress HNSW index: {}", e);
                return HnswLoadOutcome::Failed(format!("decompress: {e}"));
            }
        };

        if data.len() > MAX_DECOMPRESSED_SIZE {
            warn!(
                "HNSW decompressed data too large: {} bytes (max {})",
                data.len(),
                MAX_DECOMPRESSED_SIZE
            );
            return HnswLoadOutcome::Failed(format!(
                "decompressed data too large: {} bytes exceeds {}",
                data.len(),
                MAX_DECOMPRESSED_SIZE
            ));
        }

        match rmp_serde::from_slice::<Self>(&data) {
            Ok(index) => {
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
    fn test_cosine_point_identical() {
        let a = CosinePoint::new(vec![1.0, 0.0, 1.0]);
        let dist = a.distance(&a);
        assert!(
            dist.abs() < 1e-5,
            "distance to self should be ~0, got {dist}"
        );
    }

    #[test]
    fn test_cosine_point_orthogonal() {
        let a = CosinePoint::new(vec![1.0, 0.0]);
        let b = CosinePoint::new(vec![0.0, 1.0]);
        let dist = a.distance(&b);
        assert!(
            (dist - 1.0).abs() < 1e-5,
            "orthogonal distance should be ~1.0, got {dist}"
        );
    }

    #[test]
    fn test_cosine_point_opposite() {
        let a = CosinePoint::new(vec![1.0, 0.0]);
        let b = CosinePoint::new(vec![-1.0, 0.0]);
        let dist = a.distance(&b);
        assert!(
            (dist - 2.0).abs() < 1e-5,
            "opposite distance should be ~2.0, got {dist}"
        );
    }

    #[test]
    fn test_cosine_point_zero_vector() {
        let a = CosinePoint::new(vec![1.0, 2.0]);
        let b = CosinePoint::new(vec![0.0, 0.0]);
        let dist = a.distance(&b);
        assert_eq!(dist, 1.0, "zero vector distance should be 1.0");
    }

    #[test]
    fn test_build_empty() {
        let result = PersistedHnswIndex::build(&[]);
        assert!(result.is_none());
    }

    #[test]
    fn test_build_and_search() {
        let embeddings = vec![
            make_embedding("a", vec![1.0, 0.0, 0.0]),
            make_embedding("b", vec![0.0, 1.0, 0.0]),
            make_embedding("c", vec![0.0, 0.0, 1.0]),
            make_embedding("d", vec![0.7, 0.7, 0.0]),
        ];

        let index = PersistedHnswIndex::build(&embeddings).unwrap();
        assert_eq!(index.meta.point_count, 4);
        assert_eq!(index.meta.dimensions, 3);

        // Search for something close to "a" ([1, 0, 0])
        let results = index.search(&[0.9, 0.1, 0.0], 2, None);
        assert_eq!(results.len(), 2);
        // "a" should be the top result
        assert_eq!(results[0].chunk_id, "a");
        assert!(results[0].score > 0.9);
    }

    #[test]
    fn test_search_with_filter() {
        let embeddings = vec![
            make_embedding("a", vec![1.0, 0.0, 0.0]),
            make_embedding("b", vec![0.9, 0.1, 0.0]),
            make_embedding("c", vec![0.0, 0.0, 1.0]),
        ];

        let index = PersistedHnswIndex::build(&embeddings).unwrap();

        // Search for [1, 0, 0] but only allow "b" and "c"
        let allowed: HashSet<String> = ["b", "c"].iter().map(|s| s.to_string()).collect();
        let results = index.search(&[1.0, 0.0, 0.0], 2, Some(&allowed));

        // "a" is closest but filtered out; "b" should be top
        assert!(!results.iter().any(|r| r.chunk_id == "a"));
        assert!(results.iter().any(|r| r.chunk_id == "b"));
    }

    #[test]
    fn test_dimension_mismatch_returns_empty() {
        let embeddings = vec![make_embedding("a", vec![1.0, 0.0, 0.0])];
        let index = PersistedHnswIndex::build(&embeddings).unwrap();

        // Query with wrong dimensions
        let results = index.search(&[1.0, 0.0], 2, None);
        assert!(results.is_empty());
    }

    #[test]
    fn test_serialize_round_trip() {
        let embeddings = vec![
            make_embedding("x", vec![1.0, 0.0]),
            make_embedding("y", vec![0.0, 1.0]),
        ];

        let index = PersistedHnswIndex::build(&embeddings).unwrap();

        let dir = std::env::temp_dir().join(format!("oxideterm_hnsw_test_{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("test_hnsw.bin");

        // Save
        index.save(&path).unwrap();
        assert!(path.exists());

        // Load
        let loaded = PersistedHnswIndex::load(&path).unwrap();
        assert_eq!(loaded.meta.point_count, 2);
        assert_eq!(loaded.meta.dimensions, 2);

        // Search should produce same results
        let orig_results = index.search(&[0.9, 0.1], 1, None);
        let loaded_results = loaded.search(&[0.9, 0.1], 1, None);
        assert_eq!(orig_results[0].chunk_id, loaded_results[0].chunk_id);

        // Cleanup
        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_dir(&dir);
    }
}
