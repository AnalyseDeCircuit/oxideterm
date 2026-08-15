use std::{sync::Arc, time::Duration};

use oxideterm_public_mcp::{
    ClientRef, DomainRequest, FileSessionRef, PublicToolCall, ToolEnvelope,
};
use oxideterm_sftp::{FileInfo, FileType, ListFilter, SortOrder};
use oxideterm_ssh::{ConnectionConsumer, NodeId, NodeRouter};
use serde::Serialize;
use serde_json::json;
use sha2::{Digest, Sha256};

use super::{
    PublicMcpFileSessionRecord, PublicMcpRuntimeHandles, WorkspaceApp, finish_serialized,
    node_lease_for_client, workspaces,
};

const FILE_SESSION_CAPACITY: usize = 128;
const FILE_SESSION_CAPACITY_PER_CLIENT: usize = 32;
const FILE_LIST_DEFAULT_LIMIT: usize = 200;
const FILE_READ_DEFAULT_LIMIT: usize = 1024 * 1024;
const FILE_ARTIFACT_MAXIMUM_BYTES: u64 = 16 * 1024 * 1024;
const FILE_SESSION_ACQUIRE_TIMEOUT: Duration = Duration::from_secs(15);
const ARTIFACT_NAME_MAXIMUM_BYTES: usize = 255;

#[derive(Serialize)]
struct PublicFileProjection {
    name: String,
    path: String,
    kind: &'static str,
    size: u64,
    modified: i64,
    permissions: String,
    is_symlink: bool,
    revision: String,
}

impl WorkspaceApp {
    /// Opens a separately owned SFTP consumer without borrowing a terminal lifetime.
    pub(super) fn handle_public_mcp_files_open(&self, request: DomainRequest) {
        let PublicToolCall::FilesOpen(args) = &request.call else {
            return;
        };
        let Some(node_lease) = node_lease_for_client(
            &self.public_mcp.runtime_handles,
            &request.client_ref,
            &args.node_ref,
        ) else {
            request.finish(ToolEnvelope::failed("The node handle is unavailable"));
            return;
        };
        let file_session_ref = FileSessionRef::new();
        let consumer = ConnectionConsumer::Sftp(file_session_ref.to_string());
        {
            let mut handles = self.public_mcp.runtime_handles.lock();
            let client_count = handles
                .file_sessions
                .values()
                .filter(|record| record.client_ref == request.client_ref)
                .count();
            if handles.file_sessions.len() >= FILE_SESSION_CAPACITY
                || client_count >= FILE_SESSION_CAPACITY_PER_CLIENT
            {
                request.finish(ToolEnvelope::failed(
                    "The retained SFTP session limit has been reached",
                ));
                return;
            }
            handles.file_sessions.insert(
                file_session_ref.clone(),
                PublicMcpFileSessionRecord {
                    client_ref: request.client_ref.clone(),
                    node_id: node_lease.node_id.clone(),
                    root: None,
                    session: None,
                    physical_connection_id: None,
                    consumer: consumer.clone(),
                },
            );
        }

        let router = self.node_router.clone();
        let handles = self.public_mcp.runtime_handles.clone();
        let requested_root = args.root.clone().unwrap_or_else(|| ".".to_owned());
        let client_ref = request.client_ref.clone();
        let cancellation = request.cancellation_token();
        let node_id = node_lease.node_id;
        self.forwarding_runtime.spawn(async move {
            let resolved = tokio::select! {
                _ = cancellation.cancelled() => {
                    remove_file_session_reservation(&handles, &file_session_ref, &client_ref);
                    return;
                }
                result = router.acquire_connection_wait(
                    &node_id,
                    consumer.clone(),
                    FILE_SESSION_ACQUIRE_TIMEOUT,
                ) => result,
            };
            let resolved = match resolved {
                Ok(resolved) => resolved,
                Err(_) => {
                    remove_file_session_reservation(&handles, &file_session_ref, &client_ref);
                    request.finish(ToolEnvelope::failed("The SSH node is not ready for SFTP"));
                    return;
                }
            };
            let physical_connection_id = resolved.connection_id;
            let session = match router.acquire_sftp(&node_id).await {
                Ok(session) => session,
                Err(_) => {
                    remove_file_session_reservation(&handles, &file_session_ref, &client_ref);
                    router.release_consumer(&physical_connection_id, &consumer);
                    request.finish(ToolEnvelope::failed("The SFTP subsystem is unavailable"));
                    return;
                }
            };
            let root = {
                let session_guard = session.lock().await;
                let root = match session_guard.canonicalize(&requested_root).await {
                    Ok(root) => root,
                    Err(_) => {
                        drop(session_guard);
                        remove_file_session_reservation(&handles, &file_session_ref, &client_ref);
                        router.release_consumer(&physical_connection_id, &consumer);
                        request.finish(ToolEnvelope::failed("The SFTP root does not exist"));
                        return;
                    }
                };
                match session_guard.stat(&root).await {
                    Ok(info) if info.file_type == FileType::Directory => root,
                    _ => {
                        drop(session_guard);
                        remove_file_session_reservation(&handles, &file_session_ref, &client_ref);
                        router.release_consumer(&physical_connection_id, &consumer);
                        request.finish(ToolEnvelope::failed("The SFTP root is not a directory"));
                        return;
                    }
                }
            };
            if cancellation.is_cancelled() {
                remove_file_session_reservation(&handles, &file_session_ref, &client_ref);
                router.release_consumer(&physical_connection_id, &consumer);
                return;
            }
            let retained = {
                let mut handles = handles.lock();
                handles
                    .file_sessions
                    .get_mut(&file_session_ref)
                    .filter(|record| record.client_ref == client_ref)
                    .map(|record| {
                        record.root = Some(root.clone());
                        record.session = Some(session);
                        record.physical_connection_id = Some(physical_connection_id.clone());
                    })
                    .is_some()
            };
            if !retained {
                router.release_consumer(&physical_connection_id, &consumer);
                request.finish(ToolEnvelope::failed(
                    "The SFTP grant was revoked while opening",
                ));
                return;
            }
            finish_serialized(
                request,
                json!({ "file_session_ref": file_session_ref, "root": root }),
            );
        });
    }

    /// Releases only the SFTP consumer represented by this public handle.
    pub(super) fn handle_public_mcp_files_close(&self, request: DomainRequest) {
        let PublicToolCall::FilesClose(args) = &request.call else {
            return;
        };
        let record = {
            let mut handles = self.public_mcp.runtime_handles.lock();
            if handles
                .file_sessions
                .get(&args.file_session_ref)
                .is_some_and(|record| record.client_ref == request.client_ref)
            {
                handles.file_sessions.remove(&args.file_session_ref)
            } else {
                None
            }
        };
        let Some(record) = record else {
            request.finish(ToolEnvelope::failed("The SFTP handle is unavailable"));
            return;
        };
        for workspace in workspaces::take_file_session_workspaces(
            &self.public_mcp.runtime_handles,
            &args.file_session_ref,
        ) {
            workspace.revoke();
        }
        self.cancel_public_mcp_file_session_transfers(&args.file_session_ref);
        if let Some(connection_id) = record.physical_connection_id {
            self.node_router
                .release_consumer(&connection_id, &record.consumer);
        }
        finish_serialized(
            request,
            json!({ "closed": true, "physical_node_disconnected": false }),
        );
    }

    pub(super) fn handle_public_mcp_files_list(&self, request: DomainRequest) {
        let PublicToolCall::FilesList(args) = &request.call else {
            return;
        };
        let path = args.path.clone().unwrap_or_else(|| ".".to_owned());
        let show_hidden = args.show_hidden;
        let pattern = args.pattern.clone();
        let cursor = args.cursor as usize;
        let limit = args
            .limit
            .map_or(FILE_LIST_DEFAULT_LIMIT, |limit| limit as usize);
        self.start_public_mcp_file_job(request, move |record| async move {
            let root = ready_root(&record)?;
            let requested_path = path_from_root(root, &path);
            let session = record
                .session
                .as_ref()
                .ok_or("The SFTP handle is still opening")?
                .lock()
                .await;
            let canonical_path = session
                .canonicalize(&requested_path)
                .await
                .map_err(|_| "The remote directory is unavailable")?;
            require_path_within_root(root, &canonical_path)?;
            let entries = session
                .list_dir(
                    &canonical_path,
                    Some(ListFilter {
                        show_hidden,
                        pattern,
                        sort: SortOrder::Name,
                    }),
                )
                .await
                .map_err(|_| "The remote directory could not be listed")?;
            if entries
                .iter()
                .any(|entry| !remote_path_is_within(root, &entry.path))
            {
                return Err("The remote directory escaped its authorized root");
            }
            let next_cursor = (cursor.saturating_add(limit) < entries.len())
                .then_some(cursor.saturating_add(limit));
            let entries = entries
                .into_iter()
                .skip(cursor)
                .take(limit)
                .map(public_file_projection)
                .collect::<Vec<_>>();
            Ok(json!({ "entries": entries, "next_cursor": next_cursor }))
        });
    }

    pub(super) fn handle_public_mcp_files_stat(&self, request: DomainRequest) {
        let PublicToolCall::FilesStat(args) = &request.call else {
            return;
        };
        let path = args.path.clone();
        self.start_public_mcp_file_job(request, move |record| async move {
            let root = ready_root(&record)?;
            let requested_path = path_from_root(root, &path);
            let info = record
                .session
                .as_ref()
                .ok_or("The SFTP handle is still opening")?
                .lock()
                .await
                .stat(&requested_path)
                .await
                .map_err(|_| "The remote path is unavailable")?;
            require_path_within_root(root, &info.path)?;
            Ok(json!({ "file": public_file_projection(info) }))
        });
    }

    pub(super) fn handle_public_mcp_files_read(&self, request: DomainRequest) {
        let PublicToolCall::FilesRead(args) = &request.call else {
            return;
        };
        let path = args.path.clone();
        let offset = args.offset;
        let maximum_bytes = args
            .maximum_bytes
            .map_or(FILE_READ_DEFAULT_LIMIT, |limit| limit as usize);
        let artifact_store = self.public_mcp.state.artifacts.clone();
        let client_ref = request.client_ref.clone();
        self.start_public_mcp_file_job(request, move |record| async move {
            let root = ready_root(&record)?;
            let requested_path = path_from_root(root, &path);
            let session = record
                .session
                .as_ref()
                .ok_or("The SFTP handle is still opening")?
                .lock()
                .await;
            let canonical_path = session
                .canonicalize(&requested_path)
                .await
                .map_err(|_| "The remote file is unavailable")?;
            require_path_within_root(root, &canonical_path)?;
            let (canonical_path, total_size, bytes) = session
                .read_file_range(&canonical_path, offset, maximum_bytes)
                .await
                .map_err(|_| "The remote file range could not be read")?;
            let byte_count = bytes.len();
            let artifact = artifact_store
                .stage(
                    client_ref,
                    &bytes,
                    "application/octet-stream".to_owned(),
                    safe_artifact_name(&canonical_path),
                )
                .map_err(|_| "The remote file artifact could not be retained")?;
            let next_offset = offset
                .saturating_add(byte_count as u64)
                .lt(&total_size)
                .then_some(offset.saturating_add(byte_count as u64));
            Ok(json!({
                "artifact": artifact,
                "path": canonical_path,
                "offset": offset,
                "bytes": byte_count,
                "total_size": total_size,
                "next_offset": next_offset,
            }))
        });
    }

    pub(super) fn handle_public_mcp_files_compare(&self, request: DomainRequest) {
        let PublicToolCall::FilesCompare(args) = &request.call else {
            return;
        };
        let path = args.path.clone();
        let artifact = match self.public_mcp.state.artifacts.read_all(
            &request.client_ref,
            &args.artifact_ref,
            FILE_ARTIFACT_MAXIMUM_BYTES,
        ) {
            Ok(artifact) => artifact,
            Err(_) => {
                request.finish(ToolEnvelope::failed(
                    "The comparison artifact is unavailable",
                ));
                return;
            }
        };
        self.start_public_mcp_file_job(request, move |record| async move {
            let root = ready_root(&record)?;
            let requested_path = path_from_root(root, &path);
            let session = record
                .session
                .as_ref()
                .ok_or("The SFTP handle is still opening")?;
            let session = session.lock().await;
            let canonical_path = session
                .canonicalize(&requested_path)
                .await
                .map_err(|_| "The remote file is unavailable")?;
            require_path_within_root(root, &canonical_path)?;
            let info = session
                .stat(&canonical_path)
                .await
                .map_err(|_| "The remote file metadata is unavailable")?;
            if info.size > FILE_ARTIFACT_MAXIMUM_BYTES {
                return Err("The remote file exceeds the bounded comparison limit");
            }
            let (_, total_size, remote_bytes) = session
                .read_file_range(
                    &canonical_path,
                    0,
                    usize::try_from(FILE_ARTIFACT_MAXIMUM_BYTES).unwrap_or(usize::MAX),
                )
                .await
                .map_err(|_| "The remote file could not be read for comparison")?;
            let remote_digest = hex_digest(&remote_bytes);
            Ok(json!({
                "equal": remote_bytes.as_slice() == artifact.bytes.as_slice(),
                "remote_size": total_size,
                "artifact_size": artifact.projection.size,
                "remote_digest": remote_digest,
                "artifact_digest": artifact.projection.digest,
                "remote_revision": file_revision(&info),
            }))
        });
    }

    pub(super) fn handle_public_mcp_files_write(&self, request: DomainRequest) {
        let PublicToolCall::FilesWrite(args) = &request.call else {
            return;
        };
        let path = args.path.clone();
        let overwrite = args.overwrite;
        let expected_revision = args.expected_revision.clone();
        let artifact = match self.public_mcp.state.artifacts.read_all(
            &request.client_ref,
            &args.artifact_ref,
            FILE_ARTIFACT_MAXIMUM_BYTES,
        ) {
            Ok(artifact) => artifact,
            Err(_) => {
                request.finish(ToolEnvelope::failed("The input artifact is unavailable"));
                return;
            }
        };
        self.start_public_mcp_file_job(request, move |record| async move {
            let root = ready_root(&record)?;
            let requested_path = path_from_root(root, &path);
            let session = record
                .session
                .as_ref()
                .ok_or("The SFTP handle is still opening")?;
            let session = session.lock().await;
            let canonical_path = session
                .canonicalize_write_target(&requested_path)
                .await
                .map_err(|_| "The remote write target is invalid")?;
            require_mutable_path_within_root(root, &canonical_path)?;
            let existing = session.stat(&canonical_path).await.ok();
            require_expected_revision(existing.as_ref(), expected_revision.as_deref())?;
            if existing.is_some() && !overwrite {
                return Err("The remote file already exists and overwrite was not authorized");
            }
            let outcome = session
                .write_content(&canonical_path, &artifact.bytes)
                .await
                .map_err(|_| "The remote file could not be written")?;
            let info = session
                .stat(&canonical_path)
                .await
                .map_err(|_| "The written remote file could not be verified")?;
            Ok(json!({
                "file": public_file_projection(info),
                "atomic_write": outcome.atomic_write,
                "source_digest": artifact.projection.digest,
            }))
        });
    }

    pub(super) fn handle_public_mcp_files_move(&self, request: DomainRequest) {
        let PublicToolCall::FilesMove(args) = &request.call else {
            return;
        };
        let source_path = args.source_path.clone();
        let destination_path = args.destination_path.clone();
        let overwrite = args.overwrite;
        let expected_revision = args.expected_revision.clone();
        self.start_public_mcp_file_job(request, move |record| async move {
            let root = ready_root(&record)?;
            let session = record
                .session
                .as_ref()
                .ok_or("The SFTP handle is still opening")?;
            let session = session.lock().await;
            let source = session
                .stat(&path_from_root(root, &source_path))
                .await
                .map_err(|_| "The remote source path is unavailable")?;
            require_mutable_path_within_root(root, &source.path)?;
            require_expected_revision(Some(&source), expected_revision.as_deref())?;
            let destination = session
                .canonicalize_write_target(&path_from_root(root, &destination_path))
                .await
                .map_err(|_| "The remote destination path is invalid")?;
            require_mutable_path_within_root(root, &destination)?;
            if !overwrite && session.stat(&destination).await.is_ok() {
                return Err("The remote destination already exists");
            }
            session
                .rename(&source.path, &destination)
                .await
                .map_err(|_| "The remote path could not be moved")?;
            let moved = session
                .stat(&destination)
                .await
                .map_err(|_| "The moved remote path could not be verified")?;
            Ok(json!({ "file": public_file_projection(moved) }))
        });
    }

    pub(super) fn handle_public_mcp_files_remove(&self, request: DomainRequest) {
        let PublicToolCall::FilesRemove(args) = &request.call else {
            return;
        };
        let path = args.path.clone();
        let recursive = args.recursive;
        let expected_revision = args.expected_revision.clone();
        self.start_public_mcp_file_job(request, move |record| async move {
            let root = ready_root(&record)?;
            let session = record
                .session
                .as_ref()
                .ok_or("The SFTP handle is still opening")?;
            let session = session.lock().await;
            let info = session
                .stat(&path_from_root(root, &path))
                .await
                .map_err(|_| "The remote path is unavailable")?;
            require_mutable_path_within_root(root, &info.path)?;
            require_expected_revision(Some(&info), expected_revision.as_deref())?;
            let removed_entries = if recursive {
                session
                    .delete_recursive(&info.path)
                    .await
                    .map_err(|_| "The remote path could not be removed recursively")?
            } else {
                session
                    .delete(&info.path)
                    .await
                    .map_err(|_| "The remote path could not be removed")?;
                1
            };
            Ok(json!({ "removed": true, "removed_entries": removed_entries }))
        });
    }

    /// Refreshes the node-owned shared SFTP channel before each file operation.
    fn start_public_mcp_file_job<F, Fut>(&self, request: DomainRequest, operation: F)
    where
        F: FnOnce(PublicMcpFileSessionRecord) -> Fut + Send + 'static,
        Fut: std::future::Future<Output = Result<serde_json::Value, &'static str>> + Send + 'static,
    {
        let file_session_ref = match &request.call {
            PublicToolCall::FilesList(args) => args.file_session_ref.clone(),
            PublicToolCall::FilesStat(args) => args.file_session_ref.clone(),
            PublicToolCall::FilesRead(args) => args.file_session_ref.clone(),
            PublicToolCall::FilesCompare(args) => args.file_session_ref.clone(),
            PublicToolCall::FilesWrite(args) => args.file_session_ref.clone(),
            PublicToolCall::FilesMove(args) => args.file_session_ref.clone(),
            PublicToolCall::FilesRemove(args) => args.file_session_ref.clone(),
            _ => return,
        };
        let router = self.node_router.clone();
        let handles = self.public_mcp.runtime_handles.clone();
        let client_ref = request.client_ref.clone();
        self.forwarding_runtime.spawn(async move {
            let record =
                match refresh_file_session(&router, &handles, &client_ref, &file_session_ref).await
                {
                    Ok(record) => record,
                    Err(error) => {
                        request.finish(ToolEnvelope::failed(error));
                        return;
                    }
                };
            if request.is_cancelled() {
                return;
            }
            match operation(record).await {
                Ok(value) => finish_serialized(request, value),
                Err(error) => request.finish(ToolEnvelope::failed(error)),
            }
        });
    }
}

pub(super) fn take_client_file_sessions(
    handles: &Arc<parking_lot::Mutex<PublicMcpRuntimeHandles>>,
    client_ref: &ClientRef,
) -> Vec<PublicMcpFileSessionRecord> {
    handles
        .lock()
        .file_sessions
        .extract_if(|_, record| record.client_ref == *client_ref)
        .map(|(_, record)| record)
        .collect()
}

pub(super) fn invalidate_for_disconnected_nodes(
    handles: &mut PublicMcpRuntimeHandles,
    disconnected: &[NodeId],
) {
    handles
        .file_sessions
        .retain(|_, record| !disconnected.contains(&record.node_id));
}

pub(super) async fn refresh_file_session(
    router: &NodeRouter,
    handles: &Arc<parking_lot::Mutex<PublicMcpRuntimeHandles>>,
    client_ref: &ClientRef,
    file_session_ref: &FileSessionRef,
) -> Result<PublicMcpFileSessionRecord, &'static str> {
    let record = handles
        .lock()
        .file_sessions
        .get(file_session_ref)
        .filter(|record| record.client_ref == *client_ref)
        .cloned()
        .ok_or("The SFTP handle is unavailable")?;
    if record.root.is_none() || record.session.is_none() {
        return Err("The SFTP handle is still opening");
    }
    let resolved = router
        .acquire_connection_wait(
            &record.node_id,
            record.consumer.clone(),
            FILE_SESSION_ACQUIRE_TIMEOUT,
        )
        .await
        .map_err(|_| "The SSH node is not ready for SFTP")?;
    let physical_connection_id = resolved.connection_id;
    let session = match router.acquire_sftp(&record.node_id).await {
        Ok(session) => session,
        Err(_) => {
            if record.physical_connection_id.as_deref() != Some(&physical_connection_id) {
                router.release_consumer(&physical_connection_id, &record.consumer);
            }
            return Err("The SFTP subsystem is unavailable");
        }
    };
    let previous_connection_id = {
        let mut handles = handles.lock();
        let Some(live) = handles
            .file_sessions
            .get_mut(file_session_ref)
            .filter(|live| live.client_ref == *client_ref)
        else {
            router.release_consumer(&physical_connection_id, &record.consumer);
            return Err("The SFTP grant was revoked");
        };
        let previous = live
            .physical_connection_id
            .replace(physical_connection_id.clone());
        live.session = Some(session);
        previous.filter(|previous| previous != &physical_connection_id)
    };
    if let Some(previous_connection_id) = previous_connection_id {
        router.release_consumer(&previous_connection_id, &record.consumer);
    }
    handles
        .lock()
        .file_sessions
        .get(file_session_ref)
        .filter(|live| live.client_ref == *client_ref)
        .cloned()
        .ok_or("The SFTP grant was revoked")
}

fn remove_file_session_reservation(
    handles: &Arc<parking_lot::Mutex<PublicMcpRuntimeHandles>>,
    file_session_ref: &FileSessionRef,
    client_ref: &ClientRef,
) {
    let mut handles = handles.lock();
    if handles
        .file_sessions
        .get(file_session_ref)
        .is_some_and(|record| record.client_ref == *client_ref)
    {
        handles.file_sessions.remove(file_session_ref);
    }
}

pub(super) fn ready_root(record: &PublicMcpFileSessionRecord) -> Result<&str, &'static str> {
    record
        .root
        .as_deref()
        .ok_or("The SFTP handle is still opening")
}

pub(super) fn path_from_root(root: &str, path: &str) -> String {
    if path.starts_with('/') || path == "~" || path.starts_with("~/") {
        return path.to_owned();
    }
    if matches!(path, "." | "") {
        return root.to_owned();
    }
    if root == "/" {
        format!("/{path}")
    } else {
        format!("{}/{path}", root.trim_end_matches('/'))
    }
}

fn remote_path_is_within(root: &str, path: &str) -> bool {
    root == "/"
        || path == root
        || path
            .strip_prefix(root)
            .is_some_and(|remainder| remainder.starts_with('/'))
}

pub(super) fn require_path_within_root(root: &str, path: &str) -> Result<(), &'static str> {
    remote_path_is_within(root, path)
        .then_some(())
        .ok_or("The remote path is outside the authorized SFTP root")
}

pub(super) fn require_mutable_path_within_root(root: &str, path: &str) -> Result<(), &'static str> {
    require_path_within_root(root, path)?;
    if path == root {
        return Err("The authorized SFTP root cannot be modified");
    }
    Ok(())
}

fn require_expected_revision(
    existing: Option<&FileInfo>,
    expected_revision: Option<&str>,
) -> Result<(), &'static str> {
    match (existing, expected_revision) {
        (Some(info), Some(expected)) if file_revision(info) != expected => {
            Err("The remote path changed after the expected revision")
        }
        (None, Some(_)) => Err("The expected remote path no longer exists"),
        _ => Ok(()),
    }
}

fn public_file_projection(info: FileInfo) -> PublicFileProjection {
    PublicFileProjection {
        name: info.name.clone(),
        path: info.path.clone(),
        kind: public_file_kind(info.file_type),
        size: info.size,
        modified: info.modified,
        permissions: info.permissions.clone(),
        is_symlink: info.is_symlink,
        revision: file_revision(&info),
    }
}

fn public_file_kind(file_type: FileType) -> &'static str {
    match file_type {
        FileType::File => "file",
        FileType::Directory => "directory",
        FileType::Symlink => "symlink",
        FileType::Unknown => "unknown",
    }
}

fn file_revision(info: &FileInfo) -> String {
    let mut digest = Sha256::new();
    digest.update(info.path.as_bytes());
    digest.update([0]);
    digest.update(public_file_kind(info.file_type).as_bytes());
    digest.update(info.size.to_be_bytes());
    digest.update(info.modified.to_be_bytes());
    digest.update(info.permissions.as_bytes());
    format!("rev_{:x}", digest.finalize())
}

fn hex_digest(bytes: &[u8]) -> String {
    format!("sha256:{:x}", Sha256::digest(bytes))
}

pub(super) fn safe_artifact_name(path: &str) -> Option<String> {
    let name = path.rsplit('/').next()?;
    (!name.is_empty()
        && name.len() <= ARTIFACT_NAME_MAXIMUM_BYTES
        && !name.chars().any(char::is_control)
        && !name.contains('\\')
        && !matches!(name, "." | ".."))
    .then(|| name.to_owned())
}
