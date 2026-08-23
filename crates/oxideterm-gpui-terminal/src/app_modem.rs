use oxideterm_modem_transfer::{DetectedModemProtocol, ModemTransfer, ModemTransferDirection};
use oxideterm_terminal::TerminalModemTransferRequest;

impl TerminalPane {
    fn start_manual_modem_transfer(
        &mut self,
        protocol: DetectedModemProtocol,
        direction: ModemTransferDirection,
        cx: &mut Context<Self>,
    ) {
        let request = TerminalModemTransferRequest {
            protocol,
            direction,
        };
        let Some(transfer) = self.terminal.lock().start_modem_transfer(request.clone()) else {
            self.emit_trzsz_notice(
                self.preferences.trzsz_labels.failed_title.clone(),
                None,
                TerminalNoticeVariant::Error,
            );
            cx.notify();
            return;
        };
        self.handle_modem_transfer_prompt(request, transfer, cx);
    }

    fn handle_modem_transfer_prompt(
        &mut self,
        request: TerminalModemTransferRequest,
        transfer: ModemTransfer,
        cx: &mut Context<Self>,
    ) {
        if self.modem_prompt_active {
            transfer.stop();
            return;
        }

        self.modem_prompt_active = true;
        self.modem_connection_lost = false;
        self.modem_transfer = Some(transfer.clone());

        let receiver = match request.direction {
            ModemTransferDirection::Upload => cx.prompt_for_paths(PathPromptOptions {
                files: true,
                directories: false,
                multiple: request.protocol != oxideterm_modem_transfer::DetectedModemProtocol::Xmodem,
                prompt: Some(SharedString::from(
                    self.preferences
                        .trzsz_labels
                        .select_upload_files_title
                        .clone(),
                )),
            }),
            ModemTransferDirection::Download => cx.prompt_for_paths(PathPromptOptions {
                files: false,
                directories: true,
                multiple: false,
                prompt: Some(SharedString::from(
                    self.preferences
                        .trzsz_labels
                        .select_download_directory_title
                        .clone(),
                )),
            }),
        };

        cx.spawn(async move |weak, cx| {
            let selection = match receiver.await {
                Ok(Ok(Some(paths))) => match request.direction {
                    ModemTransferDirection::Upload => ModemPromptSelection::UploadFiles(
                        paths,
                    ),
                    ModemTransferDirection::Download => paths
                        .into_iter()
                        .next()
                        .map(ModemPromptSelection::DownloadRoot)
                        .unwrap_or(ModemPromptSelection::Cancelled),
                },
                _ => ModemPromptSelection::Cancelled,
            };

            let (event_tx, event_rx) = std::sync::mpsc::channel();
            let transfer_status = transfer.clone();
            let mut worker_handle = Some(std::thread::spawn(move || {
                run_modem_worker_job(
                    ModemWorkerJob {
                        transfer,
                        request,
                        selection,
                    },
                    event_tx,
                );
            }));
            if weak
                .update(cx, |this, _cx| {
                    // The pane owns only this transfer worker; the shared SSH
                    // connection remains owned by the node/session registry.
                    this.modem_worker = worker_handle.take();
                })
                .is_err()
            {
                transfer_status.stop();
                if let Some(worker_handle) = worker_handle.take() {
                    let _ = worker_handle.join();
                }
                return;
            }

            let mut pending_completion = None;
            loop {
                if pending_completion.is_some() {
                    let writes_drained = match weak.update(cx, |this, cx| {
                        this.terminal.lock().read_pending();
                        cx.notify();
                        this.modem_connection_lost || transfer_status.server_writes_drained()
                    }) {
                        Ok(writes_drained) => writes_drained,
                        Err(_) => {
                            transfer_status.stop();
                            break;
                        }
                    };
                    if writes_drained {
                        let event = pending_completion.take().expect("modem completion event");
                        let worker_handle = weak
                            .update(cx, |this, cx| {
                                let _ = this.handle_modem_worker_event(event, cx);
                                this.terminal.lock().finish_modem_transfer();
                                this.modem_prompt_active = false;
                                this.modem_connection_lost = false;
                                this.modem_progress = None;
                                this.modem_transfer = None;
                                cx.notify();
                                this.modem_worker.take()
                            })
                            .ok()
                            .flatten();
                        if let Some(worker_handle) = worker_handle {
                            let _ = worker_handle.join();
                        }
                        break;
                    }
                    cx.background_executor()
                        .timer(Duration::from_millis(16))
                        .await;
                    continue;
                }

                match event_rx.try_recv() {
                    Ok(ModemWorkerEvent::Progress(progress)) => {
                        if weak
                            .update(cx, |this, cx| {
                                this.update_modem_progress(progress);
                                cx.notify();
                            })
                            .is_err()
                        {
                            transfer_status.stop();
                            break;
                        }
                    }
                    Ok(event) => {
                        pending_completion = Some(event);
                    }
                    Err(std::sync::mpsc::TryRecvError::Empty) => {
                        let _ = weak.update(cx, |this, cx| {
                            this.terminal.lock().read_pending();
                            cx.notify();
                        });
                        cx.background_executor()
                            .timer(Duration::from_millis(16))
                            .await;
                    }
                    Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                        transfer_status.stop();
                        pending_completion = Some(ModemWorkerEvent::Failed(
                            "The modem worker stopped unexpectedly".to_string(),
                        ));
                    }
                }
            }
        })
        .detach();
    }

    fn handle_modem_worker_event(
        &mut self,
        event: ModemWorkerEvent,
        _cx: &mut Context<Self>,
    ) -> bool {
        match event {
            ModemWorkerEvent::Progress(progress) => {
                self.update_modem_progress(progress);
                false
            }
            ModemWorkerEvent::Completed => {
                if !self.modem_connection_lost {
                    self.emit_trzsz_notice(
                        self.preferences.trzsz_labels.completed_title.clone(),
                        None,
                        TerminalNoticeVariant::Success,
                    );
                }
                true
            }
            ModemWorkerEvent::Cancelled => {
                if !self.modem_connection_lost {
                    self.emit_trzsz_notice(
                        self.preferences.trzsz_labels.cancelled_title.clone(),
                        None,
                        TerminalNoticeVariant::Warning,
                    );
                }
                true
            }
            ModemWorkerEvent::Failed(_message) => {
                if !self.modem_connection_lost {
                    self.emit_trzsz_notice(
                        self.preferences.trzsz_labels.failed_title.clone(),
                        None,
                        TerminalNoticeVariant::Error,
                    );
                }
                true
            }
        }
    }

    fn update_modem_progress(&mut self, progress: ModemWorkerProgress) {
        let percent = progress.total_bytes.and_then(|total| {
            (total > 0).then(|| {
                ((progress.transferred_bytes as f32 / total as f32) * 100.0).clamp(0.0, 100.0)
            })
        });
        self.modem_progress = Some(ModemProgressState {
            file_name: progress.file_name,
            transferred_text: format_modem_bytes(progress.transferred_bytes),
            total_text: progress.total_bytes.map(format_modem_bytes),
            percent,
        });
    }

    fn cancel_active_modem_transfer(&mut self, cx: &mut Context<Self>) {
        if !self.modem_prompt_active {
            return;
        }
        if let Some(transfer) = &self.modem_transfer {
            transfer.stop();
        }
        self.terminal.lock().interrupt_modem_transfer();
        self.modem_progress = None;
        cx.notify();
    }

    fn notify_modem_connection_lost_if_active(&mut self) {
        if !self.modem_prompt_active || self.modem_connection_lost {
            return;
        }
        self.modem_connection_lost = true;
        if let Some(transfer) = &self.modem_transfer {
            transfer.stop();
        }
        self.terminal.lock().interrupt_modem_transfer();
        self.modem_progress = None;
        self.emit_trzsz_notice(
            self.preferences.trzsz_labels.connection_lost_title.clone(),
            None,
            TerminalNoticeVariant::Warning,
        );
    }
}

impl Drop for TerminalPane {
    fn drop(&mut self) {
        if let Some(log) = self.session_log.take() {
            // Pane teardown flushes its file sink without changing shared connection ownership.
            let _ = log.finish();
        }
        if self.modem_prompt_active || self.modem_worker.is_some() {
            // Pane teardown cancels only its logical modem consumer. It must not
            // disconnect a shared SSH node or any unrelated session consumer.
            if let Some(transfer) = &self.modem_transfer {
                transfer.stop();
            }
            let mut terminal = self.terminal.lock();
            terminal.interrupt_modem_transfer();
            // Give the live transport one synchronous chance to accept the
            // cancellation frame before this pane stops polling the session.
            terminal.read_pending();
        }
        if let Some(worker_handle) = self.modem_worker.take() {
            let _ = worker_handle.join();
        }
    }
}
