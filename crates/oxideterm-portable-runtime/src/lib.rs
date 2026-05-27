// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

//! Portable runtime detection and process-wide state.

mod detection;
pub mod keystore;
mod lock;
mod status;

pub use detection::{
    PORTABLE_CONFIG_FILENAME, PORTABLE_DEFAULT_DATA_DIRNAME, PORTABLE_KEYSTORE_FILENAME,
    PORTABLE_MARKER_FILENAME, PortableActivationKind, PortableError, PortableHostKind,
    PortableInfo, detect_portable_info_from_exe, detect_portable_info_from_exe_with_appimage,
    is_portable_mode, portable_data_dir, portable_info, portable_keystore_file_path,
};
pub use lock::{acquire_portable_instance_lock, portable_instance_lock_path};
pub use status::{
    PortableBootstrapStatus, PortableStatusSnapshot, initialize_portable_runtime,
    portable_bootstrap_status, portable_can_launch_full_app, portable_status_snapshot,
    set_portable_bootstrap_status,
};
