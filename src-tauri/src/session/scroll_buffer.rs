//! Terminal scroll buffer for backend storage and search
//!
//! Provides a thread-safe circular buffer for terminal output with:
//! - Configurable max lines (default 100,000)
//! - Efficient append and range query operations
//! - Serialization support for persistence
//! - Memory usage tracking

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use tokio::sync::RwLock;

use super::search::{search_lines, SearchOptions, SearchResult};

/// Default maximum lines to keep in buffer
/// 30K lines ≈ ~3.6 MB/session (vs 100K ≈ ~11 MB).
/// Users can override via settings; 30K covers typical interactive work.
pub const DEFAULT_MAX_LINES: usize = 30_000;

/// Single line of terminal output
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TerminalLine {
    /// Text content (ANSI codes stripped)
    pub text: String,
    /// Timestamp when line was captured (Unix milliseconds)
    pub timestamp: u64,
}

impl TerminalLine {
    /// Create a new terminal line with current timestamp
    pub fn new(text: String) -> Self {
        Self {
            text,
            timestamp: Utc::now().timestamp_millis() as u64,
        }
    }

    /// Create a terminal line with specific timestamp
    pub fn with_timestamp(text: String, timestamp: u64) -> Self {
        Self { text, timestamp }
    }
}

/// Buffer statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BufferStats {
    /// Current number of lines in buffer
    pub current_lines: usize,
    /// Total lines ever written (including scrolled out)
    pub total_lines: u64,
    /// Maximum lines configured
    pub max_lines: usize,
    /// Estimated memory usage in MB
    pub memory_usage_mb: f64,
}

/// Serialized buffer for persistence
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SerializedBuffer {
    pub lines: Vec<TerminalLine>,
    pub total_lines: u64,
    pub captured_at: DateTime<Utc>,
    pub max_lines: usize,
}

/// Thread-safe scroll buffer for terminal output
pub struct ScrollBuffer {
    /// Circular buffer of terminal lines
    lines: RwLock<VecDeque<TerminalLine>>,
    /// Maximum lines to keep
    max_lines: usize,
    /// Total lines written (including scrolled out)
    total_lines: AtomicU64,
}

impl ScrollBuffer {
    /// Create a new scroll buffer with default capacity
    pub fn new() -> Self {
        Self::with_capacity(DEFAULT_MAX_LINES)
    }

    /// Create a new scroll buffer with specified capacity
    pub fn with_capacity(max_lines: usize) -> Self {
        Self {
            lines: RwLock::new(VecDeque::with_capacity(max_lines.min(1024))),
            max_lines,
            total_lines: AtomicU64::new(0),
        }
    }

    /// Append a new line to the buffer
    /// If buffer is full, oldest line is removed
    pub async fn append(&self, line: TerminalLine) {
        let mut lines = self.lines.write().await;

        // Remove oldest line if at capacity
        if lines.len() >= self.max_lines {
            lines.pop_front();
        }

        lines.push_back(line);
        self.total_lines.fetch_add(1, Ordering::Relaxed);
    }

    /// Append multiple lines at once (more efficient)
    pub async fn append_batch(&self, new_lines: Vec<TerminalLine>) {
        if new_lines.is_empty() {
            return;
        }

        let mut lines = self.lines.write().await;
        let count = new_lines.len();

        for line in new_lines {
            if lines.len() >= self.max_lines {
                lines.pop_front();
            }
            lines.push_back(line);
        }

        self.total_lines.fetch_add(count as u64, Ordering::Relaxed);
    }

    /// Get a range of lines from the buffer
    /// Returns lines from start index up to count lines
    pub async fn get_range(&self, start: usize, count: usize) -> Vec<TerminalLine> {
        let lines = self.lines.read().await;

        lines.iter().skip(start).take(count).cloned().collect()
    }

    /// Get the last N lines from the buffer
    pub async fn tail_lines(&self, count: usize) -> Vec<TerminalLine> {
        let lines = self.lines.read().await;
        let len = lines.len();
        let start = len.saturating_sub(count);
        lines.iter().skip(start).cloned().collect()
    }

    /// Get all lines in the buffer
    pub async fn get_all(&self) -> Vec<TerminalLine> {
        let lines = self.lines.read().await;
        lines.iter().cloned().collect()
    }

    /// Get buffer statistics
    pub async fn stats(&self) -> BufferStats {
        let lines = self.lines.read().await;
        let current_count = lines.len();

        // Estimate memory usage: each line averages ~100 bytes (text + overhead)
        let avg_line_size = 100.0;
        let memory_bytes = current_count as f64 * avg_line_size;
        let memory_mb = memory_bytes / (1024.0 * 1024.0);

        BufferStats {
            current_lines: current_count,
            total_lines: self.total_lines.load(Ordering::Relaxed),
            max_lines: self.max_lines,
            memory_usage_mb: memory_mb,
        }
    }

    /// Clear all lines from the buffer
    pub async fn clear(&self) {
        let mut lines = self.lines.write().await;
        lines.clear();
        // Note: We don't reset total_lines counter - it's a historical count
    }

    /// Get current line count
    pub async fn len(&self) -> usize {
        let lines = self.lines.read().await;
        lines.len()
    }

    /// Check if buffer is empty
    pub async fn is_empty(&self) -> bool {
        let lines = self.lines.read().await;
        lines.is_empty()
    }

    /// Serialize buffer to bytes for persistence
    pub async fn save_to_bytes(&self) -> Result<Vec<u8>, rmp_serde::encode::Error> {
        let lines = self.lines.read().await;

        let serialized = SerializedBuffer {
            lines: lines.iter().cloned().collect(),
            total_lines: self.total_lines.load(Ordering::Relaxed),
            captured_at: Utc::now(),
            max_lines: self.max_lines,
        };

        rmp_serde::to_vec_named(&serialized)
    }

    /// Load buffer from serialized bytes
    pub async fn load_from_bytes(data: &[u8]) -> Result<Arc<Self>, rmp_serde::decode::Error> {
        let serialized: SerializedBuffer = rmp_serde::from_slice(data)?;

        let buffer = Self {
            lines: RwLock::new(serialized.lines.into_iter().collect()),
            max_lines: serialized.max_lines,
            total_lines: AtomicU64::new(serialized.total_lines),
        };

        Ok(Arc::new(buffer))
    }

    /// Get the maximum capacity
    pub fn max_lines(&self) -> usize {
        self.max_lines
    }

    /// Get total lines written (including scrolled out)
    pub fn total_lines(&self) -> u64 {
        self.total_lines.load(Ordering::Relaxed)
    }

    /// Search buffer contents asynchronously
    /// Uses spawn_blocking to avoid blocking the tokio runtime.
    /// Reads directly from the lock to avoid cloning all lines.
    pub async fn search(&self, options: SearchOptions) -> SearchResult {
        let lines = self.lines.read().await;
        // Collect a Vec<TerminalLine> only of the current buffer snapshot
        // so we can move it into spawn_blocking (RwLockReadGuard is !Send).
        let snapshot: Vec<TerminalLine> = lines.iter().cloned().collect();
        drop(lines); // release lock before blocking

        // Execute search in a blocking task to avoid blocking the async runtime
        tokio::task::spawn_blocking(move || search_lines(&snapshot, options))
            .await
            .unwrap_or_else(|_| SearchResult {
                matches: vec![],
                total_matches: 0,
                duration_ms: 0,
                error: Some("Search task failed".to_string()),
            })
    }
}

impl Default for ScrollBuffer {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_append_and_get() {
        let buffer = ScrollBuffer::new();

        buffer.append(TerminalLine::new("line 1".to_string())).await;
        buffer.append(TerminalLine::new("line 2".to_string())).await;
        buffer.append(TerminalLine::new("line 3".to_string())).await;

        assert_eq!(buffer.len().await, 3);

        let lines = buffer.get_range(0, 10).await;
        assert_eq!(lines.len(), 3);
        assert_eq!(lines[0].text, "line 1");
        assert_eq!(lines[2].text, "line 3");
    }

    #[tokio::test]
    async fn test_circular_buffer() {
        let buffer = ScrollBuffer::with_capacity(3);

        buffer.append(TerminalLine::new("line 1".to_string())).await;
        buffer.append(TerminalLine::new("line 2".to_string())).await;
        buffer.append(TerminalLine::new("line 3".to_string())).await;
        buffer.append(TerminalLine::new("line 4".to_string())).await; // Should evict line 1

        assert_eq!(buffer.len().await, 3);
        assert_eq!(buffer.total_lines(), 4);

        let lines = buffer.get_all().await;
        assert_eq!(lines[0].text, "line 2");
        assert_eq!(lines[2].text, "line 4");
    }

    #[tokio::test]
    async fn test_stats() {
        let buffer = ScrollBuffer::with_capacity(100);

        for i in 0..50 {
            buffer
                .append(TerminalLine::new(format!("line {}", i)))
                .await;
        }

        let stats = buffer.stats().await;
        assert_eq!(stats.current_lines, 50);
        assert_eq!(stats.total_lines, 50);
        assert_eq!(stats.max_lines, 100);
        assert!(stats.memory_usage_mb > 0.0);
    }

    #[tokio::test]
    async fn test_clear() {
        let buffer = ScrollBuffer::new();

        buffer.append(TerminalLine::new("line 1".to_string())).await;
        buffer.append(TerminalLine::new("line 2".to_string())).await;

        assert_eq!(buffer.len().await, 2);

        buffer.clear().await;

        assert_eq!(buffer.len().await, 0);
        assert_eq!(buffer.total_lines(), 2); // Historical count preserved
    }

    #[tokio::test]
    async fn test_serialization() {
        let buffer = ScrollBuffer::new();

        buffer.append(TerminalLine::new("line 1".to_string())).await;
        buffer.append(TerminalLine::new("line 2".to_string())).await;

        let bytes = buffer.save_to_bytes().await.unwrap();
        let restored = ScrollBuffer::load_from_bytes(&bytes).await.unwrap();

        assert_eq!(restored.len().await, 2);
        let lines = restored.get_all().await;
        assert_eq!(lines[0].text, "line 1");
        assert_eq!(lines[1].text, "line 2");
    }

    #[tokio::test]
    async fn test_batch_append() {
        let buffer = ScrollBuffer::with_capacity(10);

        let batch = vec![
            TerminalLine::new("line 1".to_string()),
            TerminalLine::new("line 2".to_string()),
            TerminalLine::new("line 3".to_string()),
        ];

        buffer.append_batch(batch).await;

        assert_eq!(buffer.len().await, 3);
        assert_eq!(buffer.total_lines(), 3);
    }

    #[tokio::test]
    async fn test_get_range() {
        let buffer = ScrollBuffer::new();

        for i in 0..10 {
            buffer
                .append(TerminalLine::new(format!("line {}", i)))
                .await;
        }

        let lines = buffer.get_range(3, 4).await;
        assert_eq!(lines.len(), 4);
        assert_eq!(lines[0].text, "line 3");
        assert_eq!(lines[3].text, "line 6");
    }
}
