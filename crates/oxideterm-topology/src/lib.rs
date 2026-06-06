// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

//! Pure connection topology model shared by SSH backends and native UI surfaces.
//!
//! Tauri treats the topology view as a SessionTree projection, not as a connection
//! monitor sub-feature. This crate owns that projection: wire snapshot types,
//! matrix visibility/status rules, and deterministic graph layout. GPUI only
//! renders the returned layout.

mod auto_route;
mod layout;
mod model;
mod status;

pub use auto_route::{
    AutoRouteNetworkTopology, AutoRouteNodeConfig, AutoRouteNodeInfo, AutoRouteResult,
    AutoRouteTopologyAuthType,
};
pub use layout::{
    ConnectionTopologyLayout, TOPOLOGY_CANVAS_MIN_HEIGHT, TOPOLOGY_CANVAS_MIN_WIDTH,
    TOPOLOGY_DEPTH_GAP, TOPOLOGY_LEAF_GAP, TOPOLOGY_NODE_HEIGHT, TOPOLOGY_NODE_WIDTH,
    TOPOLOGY_PADDING_X, TOPOLOGY_ROOT_Y, TopologyLayoutEdge, TopologyLayoutNode,
};
pub use model::{
    ConnectionTopologyConsumerSummary, ConnectionTopologyEdge, ConnectionTopologyNode,
    ConnectionTopologySnapshot, ConnectionTopologyStatus,
};
pub use status::{TopologyViewStatus, matrix_view_status, matrix_visible};
