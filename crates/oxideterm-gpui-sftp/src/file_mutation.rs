// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use oxideterm_i18n::I18n;
#[cfg(not(target_family = "wasm"))]
use oxideterm_sftp::MutationSession;

use super::*;

pub enum SftpDialogSubmission {
    Close,
    Wait,
    ResolveConflict,
    RenameBookmark { id: String, name: String },
    LocalNotice(TerminalNotice),
    Remote(SftpRemoteMutation),
}

pub struct SftpRemoteMutation {
    pub pane: SftpPane,
    pub clear_selection: bool,
    operation: SftpFileOperation,
    notice: MutationNotice,
}

pub enum SftpFileOperation {
    Rename { from: String, to: String },
    NewFolder(String),
    Delete(Vec<String>),
}

enum MutationDetail {
    Text(String),
    RecursiveCount(String),
}

struct MutationNotice {
    title: String,
    detail: MutationDetail,
    error: String,
}

impl MutationNotice {
    fn success_detail(self_detail: MutationDetail, count: Option<u64>) -> Option<String> {
        match self_detail {
            MutationDetail::Text(text) => Some(text),
            MutationDetail::RecursiveCount(template) => {
                count.map(|count| sftp_i18n_count(template, count.try_into().unwrap_or(usize::MAX)))
            }
        }
    }
}

impl SftpRemoteMutation {
    pub fn operation(&self) -> &SftpFileOperation {
        &self.operation
    }

    #[cfg(not(target_family = "wasm"))]
    pub async fn execute(self, session: Result<MutationSession, String>) -> SftpWorkerResult {
        let result: Result<u64, String> = async {
            let session = session?;
            match self.operation() {
                SftpFileOperation::Rename { from, to } => {
                    session.rename(from, to).await?;
                    Ok(0)
                }
                SftpFileOperation::NewFolder(path) => {
                    session.mkdir(path).await?;
                    Ok(0)
                }
                SftpFileOperation::Delete(paths) => {
                    let mut count = 0_u64;
                    for path in paths {
                        count = count.saturating_add(session.delete_recursive(path).await?);
                    }
                    Ok(count)
                }
            }
        }
        .await;
        self.complete(result)
    }

    /// The host reports the recursive deletion count for the existing completion notice.
    pub fn complete(self, result: Result<u64, String>) -> SftpWorkerResult {
        let description =
            MutationNotice::success_detail(self.notice.detail, result.as_ref().ok().copied());
        SftpWorkerResult::RemoteMutationComplete {
            result: result.map(|_| ()),
            refresh_remote: self.pane == SftpPane::Remote,
            refresh_local: self.pane == SftpPane::Local,
            toast: Some(SftpMutationToast {
                success_title: self.notice.title,
                success_description: description,
                error_title: self.notice.error,
            }),
        }
    }
}

impl SftpWorkspaceEntity {
    pub fn submit_dialog(&mut self, i18n: &I18n, cx: &mut Context<Self>) -> SftpDialogSubmission {
        use SftpDialogSubmission::*;

        let Some(dialog) = self.dialog.clone() else {
            return Wait;
        };
        let name = self.input_value(SftpInput::DialogValue).trim().to_owned();
        let (pane, operation, notice, clear_selection) = match dialog {
            SftpDialog::Rename {
                pane,
                old_name,
                old_path,
            } => {
                if name.is_empty() {
                    return Close;
                }
                let dir = self.mutation_directory(pane);
                let from = if dir.remote {
                    old_path
                        .filter(|_| pane == SftpPane::Remote)
                        .or_else(|| {
                            dir.files
                                .iter()
                                .find(|file| file.name == old_name)
                                .map(|file| file.path.clone())
                        })
                        .unwrap_or_else(|| join_sftp_path(dir.path, &old_name))
                } else {
                    join_local_path(dir.path, &old_name)
                };
                let to = if dir.remote {
                    join_sftp_path(&parent_path(&from, true), &name)
                } else {
                    join_local_path(dir.path, &name)
                };
                let notice = MutationNotice {
                    title: i18n.t("sftp.toast.renamed"),
                    detail: MutationDetail::Text(
                        i18n.t("sftp.toast.renamed_detail")
                            .replace("{{old}}", &old_name)
                            .replace("{{new}}", &name),
                    ),
                    error: i18n.t("sftp.toast.rename_failed"),
                };
                (pane, SftpFileOperation::Rename { from, to }, notice, false)
            }
            SftpDialog::NewFolder { pane, remote_path } => {
                if name.is_empty() {
                    return Close;
                }
                let dir = self.mutation_directory(pane);
                let path = if dir.remote {
                    let base = remote_path
                        .as_deref()
                        .filter(|_| pane == SftpPane::Remote)
                        .unwrap_or(dir.path);
                    join_sftp_path(base, &name)
                } else {
                    join_local_path(dir.path, &name)
                };
                let notice = MutationNotice {
                    title: i18n.t("sftp.toast.folder_created"),
                    detail: MutationDetail::Text(name),
                    error: i18n.t("sftp.toast.create_folder_failed"),
                };
                (pane, SftpFileOperation::NewFolder(path), notice, false)
            }
            SftpDialog::Delete { pane, files, paths } => {
                let dir = self.mutation_directory(pane);
                let targets: Vec<_> = match (pane, dir.remote) {
                    (SftpPane::Local, false) => files
                        .iter()
                        .map(|file| join_local_path(dir.path, file))
                        .collect(),
                    (SftpPane::Remote, true) if !paths.is_empty() => paths,
                    _ => files
                        .iter()
                        .filter_map(|name| {
                            dir.files
                                .iter()
                                .find(|file| file.name == *name)
                                .map(|file| file.path.clone())
                        })
                        .collect(),
                };
                let template = i18n.t("sftp.toast.deleted_count");
                let detail = if pane == SftpPane::Remote {
                    MutationDetail::RecursiveCount(template)
                } else {
                    MutationDetail::Text(sftp_i18n_count(template, targets.len()))
                };
                let notice = MutationNotice {
                    title: i18n.t("sftp.toast.deleted"),
                    detail,
                    error: i18n.t("sftp.toast.delete_failed"),
                };
                let clear = pane == SftpPane::Remote || !dir.remote;
                (pane, SftpFileOperation::Delete(targets), notice, clear)
            }
            SftpDialog::BookmarkRename { bookmark_id } => {
                return if name.is_empty() {
                    Wait
                } else {
                    RenameBookmark {
                        id: bookmark_id,
                        name,
                    }
                };
            }
            SftpDialog::Conflict => return ResolveConflict,
            _ => return Close,
        };

        let dir = self.mutation_directory(pane);
        if dir.remote {
            return Remote(SftpRemoteMutation {
                pane,
                clear_selection,
                operation,
                notice,
            });
        }
        let result = self.local_host.mutate(dir.path, operation);
        if result.is_ok()
            && let Ok(files) = self.local_host.list(dir.path)
        {
            self.local_files = files;
            cx.notify();
        }
        if clear_selection && self.clear_selection(pane) {
            cx.notify();
        }
        let (title, description, variant) = match result {
            Ok(()) => (
                notice.title,
                MutationNotice::success_detail(notice.detail, None),
                TerminalNoticeVariant::Success,
            ),
            Err(error) => (
                notice.error,
                Some(error.to_string()),
                TerminalNoticeVariant::Error,
            ),
        };
        LocalNotice(TerminalNotice {
            title,
            description,
            status_text: None,
            progress: None,
            variant,
        })
    }

    fn mutation_directory(&self, pane: SftpPane) -> MutationDirectory<'_> {
        match pane {
            SftpPane::Local => MutationDirectory {
                path: &self.local_path,
                files: &self.local_files,
                remote: self.pair_primary_remote_id.is_some(),
            },
            SftpPane::Remote => MutationDirectory {
                path: &self.remote_path,
                files: &self.remote_files,
                remote: true,
            },
        }
    }
}

struct MutationDirectory<'a> {
    path: &'a str,
    files: &'a [SftpFileEntry],
    remote: bool,
}

#[cfg(not(target_family = "wasm"))]
impl SftpFileOperation {
    pub(crate) fn apply_local(self, local_path: &str) -> std::io::Result<()> {
        let context =
            oxideterm_local_files::local_file_audit_context(oxideterm_audit::AuditSource::User);
        match self {
            Self::Rename { from, to } => {
                let audit = oxideterm_audit::AuditOperation::in_context(
                    context.as_ref(),
                    oxideterm_audit::AuditCategory::File,
                    "file_rename",
                    Some(&format!("{from} → {to}")),
                );
                let result = std::fs::rename(&from, &to);
                audit.result(&result);
                result
            }
            Self::NewFolder(path) => {
                let audit = oxideterm_audit::AuditOperation::in_context(
                    context.as_ref(),
                    oxideterm_audit::AuditCategory::File,
                    "file_mkdir",
                    Some(&path),
                );
                let result = std::fs::create_dir_all(&path);
                audit.result(&result);
                result
            }
            Self::Delete(files) => delete_local_files(local_path, files, context),
        }
    }
}

#[cfg(not(target_family = "wasm"))]
fn delete_local_files(
    local_path: &str,
    files: Vec<String>,
    context: Option<oxideterm_audit::AuditContext>,
) -> std::io::Result<()> {
    let count = files.len();
    let mut audit = oxideterm_audit::AuditOperation::in_context(
        context.as_ref(),
        oxideterm_audit::AuditCategory::File,
        "file_delete_recursive",
        Some(&format!("path={local_path}; requested={count}")),
    );
    let child_context = context.map(|mut context| {
        context.parent_id = audit.id().map(str::to_owned);
        context
    });
    let mut result = Ok(());
    let mut removed = 0usize;
    let mut failed_path = None;
    for path in files {
        let is_directory = std::fs::metadata(&path).is_ok_and(|metadata| metadata.is_dir());
        let child = oxideterm_audit::AuditOperation::in_context(
            child_context.as_ref(),
            oxideterm_audit::AuditCategory::File,
            if is_directory {
                "file_delete_recursive"
            } else {
                "file_delete"
            },
            Some(&path),
        );
        result = if is_directory {
            std::fs::remove_dir_all(&path)
        } else {
            std::fs::remove_file(&path)
        };
        child.result(&result);
        if result.is_err() {
            failed_path = Some(path);
            break;
        }
        removed += 1;
    }
    audit.summary(&format!("path={local_path}; requested={count}; removed={removed}; unprocessed={}; failed_path={}; nested_items=unavailable:remove_dir_all", count.saturating_sub(removed + usize::from(result.is_err())), failed_path.as_deref().unwrap_or("none")));
    audit.finish(
        if result.is_ok() {
            oxideterm_audit::AuditOutcome::Succeeded
        } else if removed > 0 {
            oxideterm_audit::AuditOutcome::Partial
        } else {
            oxideterm_audit::AuditOutcome::Failed
        },
        oxideterm_audit::AuditEvidence::Protocol,
        None,
        None,
    );

    result
}

pub fn sftp_i18n_count(template: String, count: usize) -> String {
    template.replace("{{count}}", &count.to_string())
}
