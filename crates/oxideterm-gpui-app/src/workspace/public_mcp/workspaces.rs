use std::{collections::HashSet, sync::Arc};

use oxideterm_editor_core::{BufferOffset, EditTransaction, TextBuffer, TextEdit, TextRange};
use oxideterm_ide_core::{
    AsyncIdeFileSystem, FileKind, IdeFileCheck, IdeLocation, IdeSearchQuery, SavedFileVersion,
    WriteMode,
};
use oxideterm_public_mcp::{
    ClientRef, DomainRequest, FileSessionRef, PublicToolCall, ToolEnvelope, WorkspaceRef,
};
use oxideterm_sftp::FileType;
use oxideterm_ssh::NodeId;
use serde::Serialize;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

use super::{
    PublicMcpFileSessionRecord, PublicMcpRuntimeHandles, PublicMcpWorkspaceRecord,
    PublicMcpWorkspaceRevision, WorkspaceApp, files, finish_serialized,
};

const WORKSPACE_CAPACITY: usize = 128;
const WORKSPACE_CAPACITY_PER_CLIENT: usize = 16;
const WORKSPACE_TREE_DEFAULT_LIMIT: usize = 200;
const WORKSPACE_TEXT_LIMIT_BYTES: u64 = 4 * 1024 * 1024;
const WORKSPACE_SEARCH_DEFAULT_LIMIT: u32 = 200;
const WORKSPACE_SEARCH_PREVIEW_LIMIT_BYTES: usize = 4 * 1024;

#[derive(Serialize)]
struct PublicWorkspaceTreeEntry {
    name: String,
    path: String,
    kind: &'static str,
    revision: String,
    size_bytes: Option<u64>,
    modified_millis: Option<i64>,
}

struct PreparedWorkspaceWrite {
    path: String,
    original_text: String,
    updated_text: String,
    original_version: SavedFileVersion,
}

struct AppliedWorkspaceWrite {
    path: String,
    original_text: String,
    written_version: SavedFileVersion,
}

impl WorkspaceApp {
    /// Derives a headless IDE owner from an already authorized canonical SFTP root.
    pub(super) fn handle_public_mcp_workspace_mount(
        &self,
        request: DomainRequest,
        cx: &gpui::Context<Self>,
    ) {
        let PublicToolCall::WorkspaceMount(args) = &request.call else {
            return;
        };
        let file_session_ref = args.file_session_ref.clone();
        let requested_root = args.root.clone().unwrap_or_else(|| ".".to_owned());
        let client_ref = request.client_ref.clone();
        let owner = self.ai_entity.read(cx).agent_fs().scoped_owner();
        let workspace_ref = WorkspaceRef::new();
        {
            let handles = self.public_mcp.runtime_handles.lock();
            let client_count = handles
                .workspaces
                .values()
                .filter(|record| record.client_ref == client_ref)
                .count();
            if handles.workspaces.len() >= WORKSPACE_CAPACITY
                || client_count >= WORKSPACE_CAPACITY_PER_CLIENT
            {
                request.finish(ToolEnvelope::failed(
                    "The retained IDE workspace limit has been reached",
                ));
                return;
            }
        }

        let router = self.node_router.clone();
        let handles = self.public_mcp.runtime_handles.clone();
        let cancellation = request.cancellation_token();
        self.forwarding_runtime.spawn(async move {
            let file_record = match files::refresh_file_session(
                &router,
                &handles,
                &client_ref,
                &file_session_ref,
            )
            .await
            {
                Ok(record) => record,
                Err(error) => {
                    request.finish(ToolEnvelope::failed(error));
                    return;
                }
            };
            let file_root = match files::ready_root(&file_record) {
                Ok(root) => root,
                Err(error) => {
                    request.finish(ToolEnvelope::failed(error));
                    return;
                }
            };
            let Some(session) = file_record.session.as_ref() else {
                request.finish(ToolEnvelope::failed("The SFTP handle is still opening"));
                return;
            };
            let canonical_root = {
                let session = session.lock().await;
                let candidate = files::path_from_root(file_root, &requested_root);
                let canonical = match session.canonicalize(&candidate).await {
                    Ok(path) => path,
                    Err(_) => {
                        request.finish(ToolEnvelope::failed(
                            "The IDE workspace root is unavailable",
                        ));
                        return;
                    }
                };
                if files::require_path_within_root(file_root, &canonical).is_err() {
                    request.finish(ToolEnvelope::failed(
                        "The IDE workspace root is outside the authorized SFTP root",
                    ));
                    return;
                }
                match session.stat(&canonical).await {
                    Ok(info) if info.file_type == FileType::Directory => canonical,
                    _ => {
                        request.finish(ToolEnvelope::failed(
                            "The IDE workspace root is not a directory",
                        ));
                        return;
                    }
                }
            };
            let project = match owner
                .open_project(file_record.node_id.0.clone(), canonical_root.clone())
                .await
            {
                Ok(project) => project,
                Err(_) => {
                    owner.release_all_ide_consumers();
                    request.finish(ToolEnvelope::failed(
                        "The remote IDE workspace could not be opened",
                    ));
                    return;
                }
            };
            let capabilities = owner.capabilities();
            if cancellation.is_cancelled() {
                owner.release_all_ide_consumers();
                return;
            }
            let retained = {
                let mut handles = handles.lock();
                let file_session_is_live = handles
                    .file_sessions
                    .get(&file_session_ref)
                    .is_some_and(|record| record.client_ref == client_ref);
                let client_workspace_count = handles
                    .workspaces
                    .values()
                    .filter(|record| record.client_ref == client_ref)
                    .count();
                let capacity_available = handles.workspaces.len() < WORKSPACE_CAPACITY
                    && client_workspace_count < WORKSPACE_CAPACITY_PER_CLIENT;
                if !file_session_is_live || !capacity_available {
                    false
                } else {
                    handles.workspaces.insert(
                        workspace_ref.clone(),
                        PublicMcpWorkspaceRecord {
                            client_ref: client_ref.clone(),
                            file_session_ref,
                            node_id: file_record.node_id,
                            root: canonical_root,
                            owner: owner.clone(),
                            revisions: Arc::new(parking_lot::Mutex::new(Default::default())),
                            cancellation: tokio_util::sync::CancellationToken::new(),
                            edit_cancellation: tokio_util::sync::CancellationToken::new(),
                        },
                    );
                    true
                }
            };
            if !retained {
                owner.release_all_ide_consumers();
                request.finish(ToolEnvelope::failed(
                    "The SFTP grant was revoked or the workspace limit was reached while mounting",
                ));
                return;
            }
            finish_serialized(
                request,
                json!({
                    "workspace_ref": workspace_ref,
                    "root": ".",
                    "name": project.name,
                    "is_git_repository": project.is_git_repo,
                    "git_branch": project.git_branch,
                    "capabilities": {
                        "directory_listing": capabilities.directory_listing,
                        "conflict_detection": capabilities.conflict_detection,
                        "structured_text_edits": true,
                        "atomic_file_write": capabilities.atomic_write,
                        "atomic_multi_file_edit": false,
                    },
                }),
            );
        });
    }

    pub(super) fn handle_public_mcp_workspace_tree(&self, request: DomainRequest) {
        let PublicToolCall::WorkspaceTree(args) = &request.call else {
            return;
        };
        let requested_path = args.path.clone().unwrap_or_else(|| ".".to_owned());
        let cursor = args.cursor as usize;
        let limit = args
            .limit
            .map_or(WORKSPACE_TREE_DEFAULT_LIMIT, |limit| limit as usize);
        self.start_public_mcp_workspace_job(request, move |record, file_record| async move {
            let canonical =
                canonical_workspace_path(&record, &file_record, &requested_path).await?;
            let entries = record
                .owner
                .list_dir(&IdeLocation::remote(
                    record.node_id.0.clone(),
                    canonical.clone(),
                ))
                .await
                .map_err(|_| "The remote IDE directory could not be listed".to_owned())?;
            let mut public_entries = Vec::with_capacity(entries.len());
            for entry in entries {
                let IdeLocation::Remote { path, .. } = entry.location else {
                    return Err("The IDE provider returned an invalid location".to_owned());
                };
                let path = normalize_search_path(&canonical, &path);
                files::require_path_within_root(&record.root, &path).map_err(str::to_owned)?;
                let public_revision = workspace_revision(&path, &entry.version);
                record.revisions.lock().insert(
                    path.clone(),
                    PublicMcpWorkspaceRevision {
                        public_revision: public_revision.clone(),
                        version: entry.version.clone(),
                    },
                );
                public_entries.push(PublicWorkspaceTreeEntry {
                    name: entry.name,
                    path: workspace_relative_path(&record.root, &path),
                    kind: public_workspace_kind(entry.kind),
                    revision: public_revision,
                    size_bytes: entry.version.size_bytes,
                    modified_millis: entry.version.modified_millis,
                });
            }
            public_entries.sort_by(|left, right| left.name.cmp(&right.name));
            let next_cursor = (cursor.saturating_add(limit) < public_entries.len())
                .then_some(cursor.saturating_add(limit));
            let entries = public_entries
                .into_iter()
                .skip(cursor)
                .take(limit)
                .collect::<Vec<_>>();
            Ok(json!({ "entries": entries, "next_cursor": next_cursor }))
        });
    }

    pub(super) fn handle_public_mcp_workspace_read(&self, request: DomainRequest) {
        let PublicToolCall::WorkspaceRead(args) = &request.call else {
            return;
        };
        let requested_path = args.path.clone();
        self.start_public_mcp_workspace_job(request, move |record, file_record| async move {
            let canonical =
                canonical_workspace_path(&record, &file_record, &requested_path).await?;
            require_editable_file(&record, &canonical).await?;
            let data = record
                .owner
                .read_file(&IdeLocation::remote(
                    record.node_id.0.clone(),
                    canonical.clone(),
                ))
                .await
                .map_err(|_| "The remote IDE file could not be read".to_owned())?;
            if data.text.len() as u64 > WORKSPACE_TEXT_LIMIT_BYTES {
                return Err("The remote IDE file exceeds the supported text limit".to_owned());
            }
            let revision = remember_workspace_revision(&record, &canonical, data.version.clone());
            Ok(json!({
                "path": workspace_relative_path(&record.root, &canonical),
                "text": data.text,
                "revision": revision,
                "size_bytes": data.version.size_bytes,
                "modified_millis": data.version.modified_millis,
            }))
        });
    }

    pub(super) fn handle_public_mcp_workspace_apply_edits(&self, request: DomainRequest) {
        let PublicToolCall::WorkspaceApplyEdits(args) = &request.call else {
            return;
        };
        let file_edits = args.files.clone();
        self.start_public_mcp_workspace_job(request, move |record, file_record| async move {
            // Preflight every file before the first remote write so stale revisions
            // and invalid UTF-8 byte ranges cannot create avoidable partial edits.
            let mut prepared = Vec::with_capacity(file_edits.len());
            let mut distinct_paths = HashSet::with_capacity(file_edits.len());
            for file in file_edits {
                let canonical = canonical_workspace_path(&record, &file_record, &file.path).await?;
                if !distinct_paths.insert(canonical.clone()) {
                    return Err("A workspace edit may target each file only once".to_owned());
                }
                require_editable_file(&record, &canonical).await?;
                let observed = record
                    .revisions
                    .lock()
                    .get(&canonical)
                    .cloned()
                    .ok_or_else(|| "Read the remote IDE file before applying edits".to_owned())?;
                if observed.public_revision != file.expected_revision {
                    return Err("The expected IDE file revision is stale".to_owned());
                }
                let location = IdeLocation::remote(record.node_id.0.clone(), canonical.clone());
                let current = record
                    .owner
                    .read_file(&location)
                    .await
                    .map_err(|_| "The remote IDE file could not be read before editing".to_owned())?;
                if observed.version != current.version
                    || workspace_revision(&canonical, &current.version) != file.expected_revision
                {
                    return Err("The remote IDE file changed before editing".to_owned());
                }
                let mut buffer = TextBuffer::new(current.text.clone());
                let edits = file
                    .edits
                    .into_iter()
                    .map(|mut edit| {
                        let replacement = std::mem::take(&mut *edit.replacement);
                        TextEdit::new(
                            TextRange::new(
                                BufferOffset(edit.start_byte as usize),
                                BufferOffset(edit.end_byte as usize),
                            ),
                            replacement,
                        )
                    })
                    .collect::<Vec<_>>();
                buffer
                    .apply_transaction(EditTransaction::new(edits))
                    .map_err(|_| "A workspace edit contains an invalid or overlapping range".to_owned())?;
                let updated_text = buffer.text();
                if updated_text.len() as u64 > WORKSPACE_TEXT_LIMIT_BYTES {
                    return Err("The edited IDE file exceeds the supported text limit".to_owned());
                }
                prepared.push(PreparedWorkspaceWrite {
                    path: canonical,
                    original_text: current.text,
                    updated_text,
                    original_version: current.version,
                });
            }

            let mut applied = Vec::with_capacity(prepared.len());
            for write in prepared {
                let location = IdeLocation::remote(record.node_id.0.clone(), write.path.clone());
                let written_version = match record
                    .owner
                    .write_file(
                        &location,
                        &write.updated_text,
                        Some(&write.original_version),
                        WriteMode::AtomicReplace,
                    )
                    .await
                {
                    Ok(version) => version,
                    Err(_) => {
                        // Remote servers do not provide a multi-file transaction. Compensate
                        // earlier writes only when their just-written versions still match.
                        let rollback_complete = rollback_workspace_writes(&record, &applied).await;
                        return Err(if rollback_complete {
                            "The remote IDE edit conflicted; earlier writes were restored".to_owned()
                        } else {
                            "The remote IDE edit failed and one or more earlier files may remain changed"
                                .to_owned()
                        });
                    }
                };
                applied.push(AppliedWorkspaceWrite {
                    path: write.path,
                    original_text: write.original_text,
                    written_version,
                });
            }
            let files = applied
                .into_iter()
                .map(|write| {
                    let revision = remember_workspace_revision(
                        &record,
                        &write.path,
                        write.written_version,
                    );
                    json!({
                        "path": workspace_relative_path(&record.root, &write.path),
                        "revision": revision,
                    })
                })
                .collect::<Vec<_>>();
            Ok(json!({ "applied": true, "files": files, "atomic_across_files": false }))
        });
    }

    pub(super) fn handle_public_mcp_workspace_search(&self, request: DomainRequest) {
        let PublicToolCall::WorkspaceSearch(args) = &request.call else {
            return;
        };
        let pattern = args.pattern.clone();
        let requested_root = args.root.clone().unwrap_or_else(|| ".".to_owned());
        let case_sensitive = args.case_sensitive;
        let maximum_results = args
            .maximum_results
            .unwrap_or(WORKSPACE_SEARCH_DEFAULT_LIMIT);
        self.start_public_mcp_workspace_job(request, move |record, file_record| async move {
            let search_root = canonical_workspace_path(&record, &file_record, &requested_root).await?;
            let matches = record
                .owner
                .search_project(
                    record.node_id.0.clone(),
                    IdeSearchQuery {
                        pattern,
                        root_path: search_root.clone(),
                        case_sensitive,
                        regex: false,
                        include_globs: Vec::new(),
                        exclude_globs: Vec::new(),
                        include_hidden: false,
                        max_results: maximum_results,
                        stale_token: 0,
                    },
                )
                .await
                .map_err(|_| "The remote IDE search could not be completed".to_owned())?;
            let mut public_matches = Vec::with_capacity(matches.len());
            for found in matches {
                let path = normalize_search_path(&search_root, &found.path);
                files::require_path_within_root(&record.root, &path).map_err(str::to_owned)?;
                public_matches.push(json!({
                    "path": workspace_relative_path(&record.root, &path),
                    "line": found.line,
                    "column": found.column,
                    "preview": truncate_utf8(&found.preview, WORKSPACE_SEARCH_PREVIEW_LIMIT_BYTES),
                    "match_start": found.match_start,
                    "match_end": found.match_end,
                }));
            }
            Ok(json!({ "matches": public_matches, "truncated": public_matches.len() >= maximum_results as usize }))
        });
    }

    pub(super) fn handle_public_mcp_workspace_close(&self, request: DomainRequest) {
        let PublicToolCall::WorkspaceClose(args) = &request.call else {
            return;
        };
        let record = {
            let mut handles = self.public_mcp.runtime_handles.lock();
            if handles
                .workspaces
                .get(&args.workspace_ref)
                .is_some_and(|record| record.client_ref == request.client_ref)
            {
                handles.workspaces.remove(&args.workspace_ref)
            } else {
                None
            }
        };
        let Some(record) = record else {
            request.finish(ToolEnvelope::failed(
                "The IDE workspace handle is unavailable",
            ));
            return;
        };
        record.revoke();
        finish_serialized(
            request,
            json!({ "closed": true, "physical_node_disconnected": false }),
        );
    }

    fn start_public_mcp_workspace_job<F, Fut>(&self, request: DomainRequest, operation: F)
    where
        F: FnOnce(PublicMcpWorkspaceRecord, PublicMcpFileSessionRecord) -> Fut + Send + 'static,
        Fut: std::future::Future<Output = Result<Value, String>> + Send + 'static,
    {
        let workspace_ref = match &request.call {
            PublicToolCall::WorkspaceTree(args) => args.workspace_ref.clone(),
            PublicToolCall::WorkspaceRead(args) => args.workspace_ref.clone(),
            PublicToolCall::WorkspaceApplyEdits(args) => args.workspace_ref.clone(),
            PublicToolCall::WorkspaceSearch(args) => args.workspace_ref.clone(),
            _ => return,
        };
        let record = self
            .public_mcp
            .runtime_handles
            .lock()
            .workspaces
            .get(&workspace_ref)
            .filter(|record| record.client_ref == request.client_ref)
            .cloned();
        let Some(record) = record else {
            request.finish(ToolEnvelope::failed(
                "The IDE workspace handle is unavailable",
            ));
            return;
        };
        let router = self.node_router.clone();
        let handles = self.public_mcp.runtime_handles.clone();
        let client_ref = request.client_ref.clone();
        let request_cancellation = request.cancellation_token();
        let workspace_cancellation = record.cancellation.clone();
        let edit_cancellation = matches!(&request.call, PublicToolCall::WorkspaceApplyEdits(_))
            .then(|| record.edit_cancellation.clone());
        self.forwarding_runtime.spawn(async move {
            let file_record = match files::refresh_file_session(
                &router,
                &handles,
                &client_ref,
                &record.file_session_ref,
            )
            .await
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
            // Revocation must prevent a cloned job from reacquiring its IDE consumer.
            let result = tokio::select! {
                biased;
                _ = workspace_cancellation.cancelled() => return,
                _ = async {
                    match edit_cancellation {
                        Some(cancellation) => cancellation.cancelled().await,
                        None => std::future::pending::<()>().await,
                    }
                } => return,
                _ = request_cancellation.cancelled() => return,
                result = operation(record, file_record) => result,
            };
            match result {
                Ok(value) => finish_serialized(request, value),
                Err(error) => request.finish(ToolEnvelope::failed(error)),
            }
        });
    }
}

async fn canonical_workspace_path(
    workspace: &PublicMcpWorkspaceRecord,
    file_record: &PublicMcpFileSessionRecord,
    path: &str,
) -> Result<String, String> {
    let session = file_record
        .session
        .as_ref()
        .ok_or_else(|| "The SFTP handle is still opening".to_owned())?
        .lock()
        .await;
    let candidate = files::path_from_root(&workspace.root, path);
    let canonical = session
        .canonicalize(&candidate)
        .await
        .map_err(|_| "The remote IDE path is unavailable".to_owned())?;
    files::require_path_within_root(&workspace.root, &canonical).map_err(str::to_owned)?;
    Ok(canonical)
}

async fn require_editable_file(
    workspace: &PublicMcpWorkspaceRecord,
    path: &str,
) -> Result<(), String> {
    match workspace
        .owner
        .check_file(workspace.node_id.0.clone(), path.to_owned())
        .await
        .map_err(|_| "The remote IDE file could not be inspected".to_owned())?
    {
        IdeFileCheck::Editable { size, .. } if size <= WORKSPACE_TEXT_LIMIT_BYTES => Ok(()),
        IdeFileCheck::Editable { .. } | IdeFileCheck::TooLarge { .. } => {
            Err("The remote IDE file exceeds the supported text limit".to_owned())
        }
        IdeFileCheck::Binary => Err("Binary files cannot be read as IDE text".to_owned()),
        IdeFileCheck::NotEditable { .. } => Err("The remote IDE file is not editable".to_owned()),
    }
}

async fn rollback_workspace_writes(
    workspace: &PublicMcpWorkspaceRecord,
    applied: &[AppliedWorkspaceWrite],
) -> bool {
    let mut complete = true;
    for write in applied.iter().rev() {
        let location = IdeLocation::remote(workspace.node_id.0.clone(), write.path.clone());
        match workspace
            .owner
            .write_file(
                &location,
                &write.original_text,
                Some(&write.written_version),
                WriteMode::AtomicReplace,
            )
            .await
        {
            Ok(restored_version) => {
                remember_workspace_revision(workspace, &write.path, restored_version);
            }
            Err(_) => complete = false,
        }
    }
    complete
}

fn remember_workspace_revision(
    workspace: &PublicMcpWorkspaceRecord,
    path: &str,
    version: SavedFileVersion,
) -> String {
    let public_revision = workspace_revision(path, &version);
    workspace.revisions.lock().insert(
        path.to_owned(),
        PublicMcpWorkspaceRevision {
            public_revision: public_revision.clone(),
            version,
        },
    );
    public_revision
}

fn workspace_revision(path: &str, version: &SavedFileVersion) -> String {
    let mut digest = Sha256::new();
    digest.update(path.as_bytes());
    digest.update([0]);
    digest.update(version.size_bytes.unwrap_or(u64::MAX).to_be_bytes());
    digest.update(version.modified_millis.unwrap_or(i64::MIN).to_be_bytes());
    if let Some(etag) = &version.etag {
        digest.update(etag.as_bytes());
    }
    format!("rev_{:x}", digest.finalize())
}

fn workspace_relative_path(root: &str, path: &str) -> String {
    if path == root {
        return ".".to_owned();
    }
    path.strip_prefix(root)
        .unwrap_or(path)
        .trim_start_matches('/')
        .to_owned()
}

fn public_workspace_kind(kind: FileKind) -> &'static str {
    match kind {
        FileKind::File => "file",
        FileKind::Directory => "directory",
        FileKind::Symlink => "symlink",
        FileKind::Other => "other",
    }
}

fn normalize_search_path(root: &str, path: &str) -> String {
    if path.starts_with('/') || path == root {
        path.to_owned()
    } else {
        files::path_from_root(root, path.trim_start_matches("./"))
    }
}

fn truncate_utf8(value: &str, maximum_bytes: usize) -> &str {
    if value.len() <= maximum_bytes {
        return value;
    }
    let mut boundary = maximum_bytes;
    while !value.is_char_boundary(boundary) {
        boundary -= 1;
    }
    &value[..boundary]
}

pub(super) fn take_client_workspaces(
    handles: &Arc<parking_lot::Mutex<PublicMcpRuntimeHandles>>,
    client_ref: &ClientRef,
) -> Vec<PublicMcpWorkspaceRecord> {
    handles
        .lock()
        .workspaces
        .extract_if(|_, record| record.client_ref == *client_ref)
        .map(|(_, record)| record)
        .collect()
}

pub(super) fn take_file_session_workspaces(
    handles: &Arc<parking_lot::Mutex<PublicMcpRuntimeHandles>>,
    file_session_ref: &FileSessionRef,
) -> Vec<PublicMcpWorkspaceRecord> {
    handles
        .lock()
        .workspaces
        .extract_if(|_, record| record.file_session_ref == *file_session_ref)
        .map(|(_, record)| record)
        .collect()
}

pub(super) fn take_disconnected_workspaces(
    handles: &mut PublicMcpRuntimeHandles,
    disconnected: &[NodeId],
) -> Vec<PublicMcpWorkspaceRecord> {
    handles
        .workspaces
        .extract_if(|_, record| disconnected.contains(&record.node_id))
        .map(|(_, record)| record)
        .collect()
}
