// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use std::time::Duration;

pub(super) const NATIVE_PLUGIN_LIFECYCLE_TIMEOUT: Duration = Duration::from_secs(5);
pub(super) const NATIVE_PLUGIN_TERMINAL_HOOK_TIMEOUT: Duration = Duration::from_millis(5);
pub(super) const NATIVE_PLUGIN_DELIVERY_POLL_INTERVAL: Duration = Duration::from_millis(80);
pub(super) const NATIVE_PLUGIN_TRANSFER_PROGRESS_INTERVAL: Duration = Duration::from_millis(500);
pub(super) const NATIVE_PLUGIN_PROFILER_METRICS_INTERVAL: Duration = Duration::from_secs(1);
pub(super) const NATIVE_PLUGIN_TOAST_TTL: Duration = Duration::from_secs(4);
pub(super) const NATIVE_PLUGIN_HTTP_BODY_LIMIT: usize = 10 * 1024 * 1024;

pub(super) use oxideterm_plugin_host_api::capabilities::{
    NATIVE_PLUGIN_CAPABILITY_FILESYSTEM_READ, NATIVE_PLUGIN_CAPABILITY_FILESYSTEM_WRITE,
    NATIVE_PLUGIN_CAPABILITY_NETWORK_FORWARD,
};

pub(super) const NATIVE_PLUGIN_API_COMMAND_SSH_POOL_STATS: &str = "ssh_get_pool_stats";
pub(super) const NATIVE_PLUGIN_API_COMMAND_LIST_CONNECTIONS: &str = "list_connections";
pub(super) const NATIVE_PLUGIN_API_COMMAND_GET_APP_VERSION: &str = "get_app_version";
pub(super) const NATIVE_PLUGIN_API_COMMAND_GET_SYSTEM_INFO: &str = "get_system_info";
pub(super) const NATIVE_PLUGIN_API_COMMAND_SFTP_CANCEL_TRANSFER: &str = "sftp_cancel_transfer";
pub(super) const NATIVE_PLUGIN_API_COMMAND_SFTP_PAUSE_TRANSFER: &str = "sftp_pause_transfer";
pub(super) const NATIVE_PLUGIN_API_COMMAND_SFTP_RESUME_TRANSFER: &str = "sftp_resume_transfer";
pub(super) const NATIVE_PLUGIN_API_COMMAND_SFTP_TRANSFER_STATS: &str = "sftp_transfer_stats";
pub(super) const NATIVE_PLUGIN_API_COMMAND_NODE_SFTP_INIT: &str = "node_sftp_init";
pub(super) const NATIVE_PLUGIN_API_COMMAND_NODE_SFTP_LIST_DIR: &str = "node_sftp_list_dir";
pub(super) const NATIVE_PLUGIN_API_COMMAND_NODE_SFTP_STAT: &str = "node_sftp_stat";
pub(super) const NATIVE_PLUGIN_API_COMMAND_NODE_SFTP_PREVIEW: &str = "node_sftp_preview";
pub(super) const NATIVE_PLUGIN_API_COMMAND_NODE_SFTP_WRITE: &str = "node_sftp_write";
pub(super) const NATIVE_PLUGIN_API_COMMAND_NODE_SFTP_DOWNLOAD: &str = "node_sftp_download";
pub(super) const NATIVE_PLUGIN_API_COMMAND_NODE_SFTP_UPLOAD: &str = "node_sftp_upload";
pub(super) const NATIVE_PLUGIN_API_COMMAND_NODE_SFTP_MKDIR: &str = "node_sftp_mkdir";
pub(super) const NATIVE_PLUGIN_API_COMMAND_NODE_SFTP_DELETE: &str = "node_sftp_delete";
pub(super) const NATIVE_PLUGIN_API_COMMAND_NODE_SFTP_DELETE_RECURSIVE: &str =
    "node_sftp_delete_recursive";
pub(super) const NATIVE_PLUGIN_API_COMMAND_NODE_SFTP_RENAME: &str = "node_sftp_rename";
pub(super) const NATIVE_PLUGIN_API_COMMAND_NODE_SFTP_DOWNLOAD_DIR: &str = "node_sftp_download_dir";
pub(super) const NATIVE_PLUGIN_API_COMMAND_NODE_SFTP_UPLOAD_DIR: &str = "node_sftp_upload_dir";
pub(super) const NATIVE_PLUGIN_API_COMMAND_NODE_SFTP_TAR_PROBE: &str = "node_sftp_tar_probe";
pub(super) const NATIVE_PLUGIN_API_COMMAND_NODE_SFTP_TAR_UPLOAD: &str = "node_sftp_tar_upload";
pub(super) const NATIVE_PLUGIN_API_COMMAND_NODE_SFTP_TAR_DOWNLOAD: &str = "node_sftp_tar_download";
pub(super) const NATIVE_PLUGIN_API_COMMAND_LIST_PORT_FORWARDS: &str = "list_port_forwards";
pub(super) const NATIVE_PLUGIN_API_COMMAND_CREATE_PORT_FORWARD: &str = "create_port_forward";
pub(super) const NATIVE_PLUGIN_API_COMMAND_STOP_PORT_FORWARD: &str = "stop_port_forward";
pub(super) const NATIVE_PLUGIN_API_COMMAND_DELETE_PORT_FORWARD: &str = "delete_port_forward";
pub(super) const NATIVE_PLUGIN_API_COMMAND_RESTART_PORT_FORWARD: &str = "restart_port_forward";
pub(super) const NATIVE_PLUGIN_API_COMMAND_UPDATE_PORT_FORWARD: &str = "update_port_forward";
pub(super) const NATIVE_PLUGIN_API_COMMAND_GET_PORT_FORWARD_STATS: &str = "get_port_forward_stats";
pub(super) const NATIVE_PLUGIN_API_COMMAND_STOP_ALL_FORWARDS: &str = "stop_all_forwards";
pub(super) const NATIVE_PLUGIN_API_COMMAND_PLUGIN_HTTP_REQUEST: &str = "plugin_http_request";

// Keep the documented api.invoke adapter surface in one place so tests can
// detect a command that is listed but not dispatched through a native owner.
#[cfg(test)]
pub(super) fn native_plugin_supported_backend_commands() -> &'static [&'static str] {
    &[
        NATIVE_PLUGIN_API_COMMAND_SSH_POOL_STATS,
        NATIVE_PLUGIN_API_COMMAND_LIST_CONNECTIONS,
        NATIVE_PLUGIN_API_COMMAND_GET_APP_VERSION,
        NATIVE_PLUGIN_API_COMMAND_GET_SYSTEM_INFO,
        NATIVE_PLUGIN_API_COMMAND_SFTP_CANCEL_TRANSFER,
        NATIVE_PLUGIN_API_COMMAND_SFTP_PAUSE_TRANSFER,
        NATIVE_PLUGIN_API_COMMAND_SFTP_RESUME_TRANSFER,
        NATIVE_PLUGIN_API_COMMAND_SFTP_TRANSFER_STATS,
        NATIVE_PLUGIN_API_COMMAND_NODE_SFTP_INIT,
        NATIVE_PLUGIN_API_COMMAND_NODE_SFTP_LIST_DIR,
        NATIVE_PLUGIN_API_COMMAND_NODE_SFTP_STAT,
        NATIVE_PLUGIN_API_COMMAND_NODE_SFTP_PREVIEW,
        NATIVE_PLUGIN_API_COMMAND_NODE_SFTP_WRITE,
        NATIVE_PLUGIN_API_COMMAND_NODE_SFTP_DOWNLOAD,
        NATIVE_PLUGIN_API_COMMAND_NODE_SFTP_UPLOAD,
        NATIVE_PLUGIN_API_COMMAND_NODE_SFTP_MKDIR,
        NATIVE_PLUGIN_API_COMMAND_NODE_SFTP_DELETE,
        NATIVE_PLUGIN_API_COMMAND_NODE_SFTP_DELETE_RECURSIVE,
        NATIVE_PLUGIN_API_COMMAND_NODE_SFTP_RENAME,
        NATIVE_PLUGIN_API_COMMAND_NODE_SFTP_DOWNLOAD_DIR,
        NATIVE_PLUGIN_API_COMMAND_NODE_SFTP_UPLOAD_DIR,
        NATIVE_PLUGIN_API_COMMAND_NODE_SFTP_TAR_PROBE,
        NATIVE_PLUGIN_API_COMMAND_NODE_SFTP_TAR_UPLOAD,
        NATIVE_PLUGIN_API_COMMAND_NODE_SFTP_TAR_DOWNLOAD,
        NATIVE_PLUGIN_API_COMMAND_LIST_PORT_FORWARDS,
        NATIVE_PLUGIN_API_COMMAND_CREATE_PORT_FORWARD,
        NATIVE_PLUGIN_API_COMMAND_STOP_PORT_FORWARD,
        NATIVE_PLUGIN_API_COMMAND_DELETE_PORT_FORWARD,
        NATIVE_PLUGIN_API_COMMAND_RESTART_PORT_FORWARD,
        NATIVE_PLUGIN_API_COMMAND_UPDATE_PORT_FORWARD,
        NATIVE_PLUGIN_API_COMMAND_GET_PORT_FORWARD_STATS,
        NATIVE_PLUGIN_API_COMMAND_STOP_ALL_FORWARDS,
        NATIVE_PLUGIN_API_COMMAND_PLUGIN_HTTP_REQUEST,
    ]
}
