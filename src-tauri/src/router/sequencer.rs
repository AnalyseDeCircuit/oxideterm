//! NodeEventSequencer — generation 管理
//!
//! 每个节点维护一个独立的单调递增计数器，用于事件有序性保证。
//! 参考: docs/OXIDE_NEXT_ARCHITECTURE.md §3.4

use std::sync::atomic::{AtomicU64, Ordering};

use dashmap::DashMap;

/// 节点事件序列器：为每个节点维护独立的 generation 计数器。
///
/// 用于：
/// - emit 事件时获取递增的 generation 值
/// - 前端初始化时获取当前 generation 与快照对齐
///
/// 线程安全：DashMap + AtomicU64，无锁递增。
pub struct NodeEventSequencer {
    counters: DashMap<String, AtomicU64>,
}

impl NodeEventSequencer {
    pub fn new() -> Self {
        Self {
            counters: DashMap::new(),
        }
    }

    /// 获取下一个 generation（原子递增），用于 emit 事件。
    ///
    /// 每次调用返回一个严格递增的值，保证事件有序。
    pub fn next(&self, node_id: &str) -> u64 {
        self.counters
            .entry(node_id.to_string())
            .or_insert_with(|| AtomicU64::new(0))
            .fetch_add(1, Ordering::SeqCst)
            + 1 // 从 1 开始，0 保留给"未初始化"
    }

    /// 获取当前 generation（不递增），用于快照查询。
    ///
    /// 前端调用 `node_get_state` 时返回此值，
    /// 用于初始化 maxGen，确保快照之前的事件不会被采纳。
    pub fn current(&self, node_id: &str) -> u64 {
        self.counters
            .get(node_id)
            .map(|c| c.load(Ordering::SeqCst))
            .unwrap_or(0)
    }

    /// 移除节点的计数器（节点永久删除时调用）。
    pub fn remove(&self, node_id: &str) {
        self.counters.remove(node_id);
    }
}

impl Default for NodeEventSequencer {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generation_starts_at_one() {
        let seq = NodeEventSequencer::new();
        assert_eq!(seq.current("node-1"), 0);
        assert_eq!(seq.next("node-1"), 1);
        assert_eq!(seq.current("node-1"), 1);
    }

    #[test]
    fn test_generation_monotonic() {
        let seq = NodeEventSequencer::new();
        let g1 = seq.next("node-1");
        let g2 = seq.next("node-1");
        let g3 = seq.next("node-1");
        assert_eq!(g1, 1);
        assert_eq!(g2, 2);
        assert_eq!(g3, 3);
    }

    #[test]
    fn test_independent_nodes() {
        let seq = NodeEventSequencer::new();
        assert_eq!(seq.next("node-a"), 1);
        assert_eq!(seq.next("node-b"), 1);
        assert_eq!(seq.next("node-a"), 2);
        assert_eq!(seq.current("node-a"), 2);
        assert_eq!(seq.current("node-b"), 1);
    }

    #[test]
    fn test_remove() {
        let seq = NodeEventSequencer::new();
        seq.next("node-1");
        seq.next("node-1");
        assert_eq!(seq.current("node-1"), 2);
        seq.remove("node-1");
        assert_eq!(seq.current("node-1"), 0);
    }
}
