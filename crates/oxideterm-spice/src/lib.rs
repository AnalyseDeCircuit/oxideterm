// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

//! Application-owned adapter for the versioned OxideSpice helper process.

mod audio;
mod frame;
mod helper_process;
mod model;
mod remote_desktop;
mod upload;
mod worker;

pub use model::{
    SpiceConnectOptions, SpiceEndpoint, SpiceHelperCommand, SpiceSasl, SpiceSecret,
    SpiceTransportSecurity, SpiceWorkerConfig, SpiceWorkerDelivery,
};
pub use oxide_spice_helper_protocol::{
    HelperAgentFeatures as SpiceAgentFeatures, HelperAgentStateKind as SpiceAgentStateKind,
    HelperAudioDataMode as SpiceAudioDataMode, HelperButtonState as SpiceButtonState,
    HelperChannelCapabilities as SpiceChannelCapabilities,
    HelperClipboardFormat as SpiceClipboardFormat,
    HelperClipboardSelection as SpiceClipboardSelection, HelperEvent as SpiceHelperEvent,
    HelperFileTransferFailure as SpiceFileTransferFailure,
    HelperFileTransferState as SpiceFileTransferState, HelperGraphicsDevice as SpiceGraphicsDevice,
    HelperKeyState as SpiceKeyState, HelperMonitor as SpiceMonitor,
    HelperMouseButton as SpiceMouseButton, HelperMouseMode as SpiceMouseMode,
    HelperNativeBackendStatus as SpiceNativeBackendStatus,
    HelperPlaybackStateKind as SpicePlaybackStateKind, HelperPortStateKind as SpicePortStateKind,
    HelperRecordStateKind as SpiceRecordStateKind, HelperRequest as SpiceWorkerRequest,
    HelperTopologyMonitor as SpiceTopologyMonitor,
    HelperUsbDeviceIdentity as SpiceUsbDeviceIdentity,
};
pub use remote_desktop::{SpiceRemoteDesktopAdapter, SpiceRemoteDesktopEventActions};
pub use upload::{SpiceFileUploadAction, SpiceFileUploadRuntime};
pub use worker::{resolve_spice_helper_command, run_spice_worker};
