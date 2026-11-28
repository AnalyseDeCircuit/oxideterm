use super::*;
use oxideterm_sftp::LocalDownloadDisposition;

pub(super) enum BatchSource {
    Pane,
    Picked(super::upload::PickedFiles),
}
pub(super) enum Batch {
    Pane(TransferDestination),
    Upload(TransferDestination, super::upload::PickedFiles),
    Device(super::device_transfer::DownloadTarget),
}

fn panes(dir: TransferDirection) -> (SftpPane, SftpPane) {
    match dir {
        TransferDirection::Upload => (SftpPane::Local, SftpPane::Remote),
        TransferDirection::Download => (SftpPane::Remote, SftpPane::Local),
    }
}

impl Browser {
    pub(super) fn queue_entries(
        &mut self,
        dir: TransferDirection,
        entries: impl IntoIterator<Item = (String, Option<String>)>,
        cx: &mut Context<Self>,
    ) {
        if dir == TransferDirection::Upload && self.device.folder.is_some() {
            let s = self.state.read(cx);
            let selected = entries
                .into_iter()
                .filter_map(|(name, _)| s.local_files.iter().find(|f| f.name == name).cloned())
                .collect();
            self.upload_device_selection(selected, cx);
            return;
        }
        let (src, _) = panes(dir);
        let state = self.state.read(cx);
        let from = match dir {
            TransferDirection::Upload => &state.local_path,
            TransferDirection::Download => &state.remote_path,
        };
        let pending: Result<Vec<_>, _> = entries
            .into_iter()
            .map(|(name, path)| {
                let path = path.unwrap_or_else(|| DemoFiles::path(from, &name));
                let source = self.fs(src).sftp_entry(&path).ok_or("ENOENT")?;
                Ok(SftpPendingTransfer {
                    name,
                    source,
                    direction: dir,
                    protocol_override: None,
                })
            })
            .collect();
        self.begin_batch(dir, pending, BatchSource::Pane, cx);
    }

    pub(super) fn begin_batch(
        &mut self,
        dir: TransferDirection,
        pending: Result<Vec<SftpPendingTransfer>, String>,
        source: BatchSource,
        cx: &mut Context<Self>,
    ) {
        let (_, dst) = panes(dir);
        let state = self.state.read(cx);
        let to = match dst {
            SftpPane::Local => &state.local_path,
            SftpPane::Remote => &state.remote_path,
        };
        if dst == SftpPane::Local && self.device.folder.is_some() {
            self.prepare_device_download(pending, cx);
            return;
        }
        let target = pending.and_then(|pending| {
            let target = TransferDestination::capture(
                &self.fs(dst).files.borrow(),
                to,
                pending.iter().map(|p| p.name.as_str()),
            )?;
            Ok((target, pending))
        });
        let (target, pending) = match target {
            Ok((_, pending)) if pending.is_empty() => return,
            Ok(pair) => pair,
            Err(error) => {
                let key = if dir == TransferDirection::Upload {
                    "sftp.toast.upload_failed"
                } else {
                    "sftp.toast.download_failed"
                };
                self.notice(
                    self.i18n.t(key),
                    Some(self.i18n.t(&error)),
                    TerminalNoticeVariant::Error,
                    cx,
                );
                return;
            }
        };
        self.reload(dst, cx);
        let batch = match source {
            BatchSource::Pane => Batch::Pane(target),
            BatchSource::Picked(files) => Batch::Upload(target, files),
        };
        self.plan_batch(dir, pending, batch, cx);
    }

    pub(super) fn plan_batch(
        &mut self,
        dir: TransferDirection,
        pending: Vec<SftpPendingTransfer>,
        batch: Batch,
        cx: &mut Context<Self>,
    ) {
        let (src, _) = panes(dir);
        let Ok(policy) = self.host.read_with(cx, |h, cx| {
            h.settings.read(cx).settings(cx).sftp.conflict_action
        }) else {
            return;
        };
        let count = pending.len();
        let launches = self.state.update(cx, |s, cx| {
            s.prepare_interactive_transfers(
                self.remote_id.clone(),
                pending,
                (!matches!(batch, Batch::Upload(..))).then_some(src),
                policy,
                TransferProtocol::Sftp,
                cx,
            )
        });
        if self.state.read(cx).conflict_state.is_some() {
            self.conflict_destination = Some(batch);
        } else {
            self.finish_transfers(dir, launches, count, batch, cx);
        }
    }

    pub(super) fn resolve_conflict(
        &mut self,
        resolution: SftpConflictResolution,
        cx: &mut Context<Self>,
    ) {
        let decision = self
            .state
            .update(cx, |s, cx| s.resolve_transfer_conflict(resolution, cx));
        if matches!(decision, SftpConflictDecision::Continue) {
            return;
        }
        self.state
            .update(cx, |s, cx| s.begin_dialog_exit(Duration::ZERO, cx));
        let Some(target) = self.conflict_destination.take() else {
            return;
        };
        if let SftpConflictDecision::Execute {
            pending_transfers,
            resolved_actions,
        } = decision
        {
            let Some(dir) = pending_transfers.first().map(|p| p.direction) else {
                return;
            };
            let count = pending_transfers.len();
            let launches = self.state.update(cx, |s, cx| {
                s.prepare_transfer_launches(
                    self.remote_id.clone(),
                    pending_transfers,
                    resolved_actions,
                    None,
                    TransferProtocol::Sftp,
                    cx,
                )
            });
            self.finish_transfers(dir, launches, count, target, cx);
        }
    }

    fn finish_transfers(
        &mut self,
        dir: TransferDirection,
        launches: Vec<SftpTransferLaunch>,
        count: usize,
        batch: Batch,
        cx: &mut Context<Self>,
    ) {
        let target = match batch {
            Batch::Upload(target, files) => {
                return self.start_uploads(launches, count, target, files, cx);
            }
            Batch::Device(target) => {
                return self.start_device_download(launches, count, target, cx);
            }
            Batch::Pane(target) => target,
        };
        let (src, dst) = panes(dir);
        let skipped = count - launches.len();
        let (mut completed, mut failed) = (0, 0);
        for launch in launches {
            let (from, to) = match dir {
                TransferDirection::Upload => (&launch.local_path, &launch.remote_path),
                TransferDirection::Download => (&launch.remote_path, &launch.local_path),
            };
            let result = target.copy(
                &self.fs(src).files,
                from,
                &self.fs(dst).files,
                to,
                launch.download_disposition == LocalDownloadDisposition::ReplaceExisting,
            );
            if result.is_ok() {
                completed += 1;
            } else {
                failed += 1;
            }
            self.finish_item(launch.id, result, cx);
        }
        if completed > 0 {
            self.files_changed(dst, cx);
        }
        self.transfer_notice(dir, completed, failed, skipped, cx);
    }

    pub(super) fn finish_item(&self, id: u64, result: Result<u64, String>, cx: &mut Context<Self>) {
        self.state.update(cx, |s, cx| {
            if let Some(item) = s.transfers.iter_mut().find(|i| i.id == id) {
                if let Ok(size) = &result {
                    item.size = *size;
                }
                oxideterm_sftp::apply_tauri_transfer_completion(
                    item,
                    &result.map(|_| ()).map_err(|e| self.i18n.t(&e)),
                );
                if let Some((batch, state)) = item.batch_id.take().map(|batch| (batch, item.state))
                {
                    s.complete_transfer_batch_item(batch, state);
                }
                cx.notify();
            }
        });
    }
}
