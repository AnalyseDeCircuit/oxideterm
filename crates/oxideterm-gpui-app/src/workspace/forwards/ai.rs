// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use super::*;

impl ForwardingWorkspaceEntity {
    pub(in crate::workspace) fn ai_snapshot_for_node(&self, node_id: &NodeId) -> serde_json::Value {
        let snapshot = self.runtime_snapshot(node_id);
        serde_json::json!({
            "rules": snapshot.rules,
            "statsByForwardId": snapshot.stats_by_forward_id,
        })
    }
}
