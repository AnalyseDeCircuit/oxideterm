// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use oxideterm_local_files::{
    PathCompletionRequest, local_path_completion_request, split_path_completion_request,
};

use super::*;

pub fn remote_path_completion_request(input: &str) -> Option<PathCompletionRequest> {
    if input.trim().is_empty() {
        return None;
    }
    let normalized = oxideterm_sftp::normalize_remote_path(input);
    split_path_completion_request(
        &normalized,
        input.ends_with('/') || normalized == "/",
        |character| character == '/',
        "/",
    )
}

pub struct SftpCompletionLoad {
    pub pane: SftpPane,
    pub generation: u64,
    pub remote_id: SftpRemoteId,
    pub parent_path: String,
}

pub enum SftpPathLoad {
    Local,
    Remote,
    PairPrimary,
}

impl SftpWorkspaceEntity {
    pub fn refresh_path_completion(
        &mut self,
        input: SftpInput,
        cx: &mut Context<Self>,
    ) -> Option<SftpCompletionLoad> {
        let pair_primary = self.pair_primary_remote_id.clone();
        let (pane, request, completion) = match input {
            SftpInput::LocalPath => (
                SftpPane::Local,
                if pair_primary.is_some() {
                    remote_path_completion_request(&self.local_path_input)
                } else {
                    local_path_completion_request(&self.local_path_input)
                },
                &mut self.local_path_completion,
            ),
            SftpInput::RemotePath => (
                SftpPane::Remote,
                remote_path_completion_request(&self.remote_path_input),
                &mut self.remote_path_completion,
            ),
            _ => return None,
        };
        let Some(request) = request else {
            completion.dismiss();
            cx.notify();
            return None;
        };
        let pending = completion.request(request);
        // A cache hit can narrow the menu without scheduling an I/O task.
        cx.notify();
        let (generation, parent_path) = pending?;

        let remote_id = match pane {
            SftpPane::Local => {
                if let Some(remote_id) = pair_primary {
                    remote_id
                } else {
                    let entries = list_local_files(&parent_path)
                        .unwrap_or_default()
                        .into_iter()
                        .map(sftp_path_completion_candidate)
                        .collect();
                    completion.apply_entries(generation, &parent_path, entries);
                    return None;
                }
            }
            SftpPane::Remote => {
                if parent_path == self.remote_path && !self.remote_loading {
                    let entries = self
                        .remote_files
                        .iter()
                        .cloned()
                        .map(sftp_path_completion_candidate)
                        .collect();
                    completion.apply_entries(generation, &parent_path, entries);
                    return None;
                }
                if let Some(remote_id) = self.current_remote_id.clone() {
                    remote_id
                } else {
                    completion.apply_entries(generation, &parent_path, Vec::new());
                    return None;
                }
            }
        };
        Some(SftpCompletionLoad {
            pane,
            generation,
            remote_id,
            parent_path,
        })
    }

    pub fn accept_completion(
        &mut self,
        pane: SftpPane,
        index: usize,
        cx: &mut Context<Self>,
    ) -> Option<SftpPathLoad> {
        let (completion, current) = match pane {
            SftpPane::Local => (&self.local_path_completion, &self.local_path),
            SftpPane::Remote => (&self.remote_path_completion, &self.remote_path),
        };
        let candidate = completion.candidate(index)?.clone();
        if candidate.is_directory {
            return Some(self.set_path(pane, candidate.path, cx));
        }
        let parent = completion.parent_path().unwrap_or(current).to_string();
        let load = self.set_path(pane, parent.clone(), cx);
        match pane {
            SftpPane::Local
                if self
                    .local_files
                    .iter()
                    .any(|entry| entry.name == candidate.name) =>
            {
                self.local_selected.insert(candidate.name.clone());
                self.local_last_selected = Some(candidate.name);
            }
            SftpPane::Remote => {
                // Match the selection to the parent listing rather than the old directory.
                self.remote_path_completion_pending_selection = Some((parent, candidate.name));
            }
            SftpPane::Local => {}
        }
        Some(load)
    }

    pub fn set_path(
        &mut self,
        pane: SftpPane,
        path: String,
        cx: &mut Context<Self>,
    ) -> SftpPathLoad {
        self.set_input_focus(None, cx);
        let load = match pane {
            SftpPane::Local if self.pair_primary_remote_id.is_some() => {
                self.apply_pair_primary_path(path);
                SftpPathLoad::PairPrimary
            }
            SftpPane::Local => {
                if let Some(remote_id) = self.current_remote_id.clone() {
                    self.local_path_by_remote.insert(remote_id, path.clone());
                }
                let audit = oxideterm_audit::AuditOperation::in_context(
                    oxideterm_local_files::local_file_audit_context(
                        oxideterm_audit::AuditSource::User,
                    )
                    .as_ref(),
                    oxideterm_audit::AuditCategory::File,
                    "file_browse",
                    Some(&path),
                );
                let result = self.apply_local_path(path);
                audit.result(&result);
                SftpPathLoad::Local
            }
            SftpPane::Remote => {
                let request = oxideterm_audit::AuditContext::current_request().or_else(|| {
                    oxideterm_audit::AuditContext::current().map(|mut context| {
                        context.source = oxideterm_audit::AuditSource::User;
                        context
                    })
                });
                self.remote_browse_request = self
                    .current_remote_id
                    .clone()
                    .zip(request)
                    .map(|(remote_id, context)| (remote_id, path.clone(), context));
                self.apply_remote_path(path);
                SftpPathLoad::Remote
            }
        };
        cx.notify();
        load
    }

    pub fn committed_path_input(&self, pane: SftpPane) -> String {
        match pane {
            SftpPane::Local if self.pair_primary_remote_id.is_some() => {
                normalize_remote_path(&self.local_path_input)
            }
            SftpPane::Local => self.local_path_input.trim().to_string(),
            SftpPane::Remote => normalize_remote_path(&self.remote_path_input),
        }
    }

    pub fn navigation_target(&self, pane: SftpPane, target: &str) -> String {
        match (pane, target) {
            (SftpPane::Local, "~") if self.pair_primary_remote_id.is_some() => {
                let sftp = self;
                sftp.pair_primary_remote_id
                    .as_ref()
                    .and_then(|remote_id| sftp.remote_home_by_remote.get(remote_id))
                    .cloned()
                    .unwrap_or_else(|| "/".to_string())
            }
            (SftpPane::Local, "~") => home_path(),
            (SftpPane::Remote, "~") => {
                let sftp = self;
                sftp.current_remote_id
                    .as_ref()
                    .and_then(|remote_id| sftp.remote_home_by_remote.get(remote_id))
                    .cloned()
                    .unwrap_or_else(|| "/".to_string())
            }
            (SftpPane::Local, "..") => {
                parent_path(&self.local_path, self.pair_primary_remote_id.is_some())
            }
            (SftpPane::Remote, "..") => parent_path(&self.remote_path, true),
            _ => target.to_string(),
        }
    }
}
