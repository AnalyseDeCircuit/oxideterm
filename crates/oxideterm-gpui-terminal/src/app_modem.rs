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
                        paths
                            .into_iter()
                            .map(|path| path.to_string_lossy().to_string())
                            .collect(),
                    ),
                    ModemTransferDirection::Download => paths
                        .into_iter()
                        .next()
                        .map(|path| ModemPromptSelection::DownloadRoot(path.to_string_lossy().to_string()))
                        .unwrap_or(ModemPromptSelection::Cancelled),
                },
                _ => ModemPromptSelection::Cancelled,
            };

            let (event_tx, event_rx) = std::sync::mpsc::channel();
            std::thread::spawn(move || {
                run_modem_worker_job(
                    ModemWorkerJob {
                        transfer,
                        request,
                        selection,
                    },
                    event_tx,
                );
            });

            loop {
                match event_rx.try_recv() {
                    Ok(event) => {
                        let mut done = false;
                        if weak
                            .update(cx, |this, cx| {
                                done = this.handle_modem_worker_event(event, cx);
                                if done {
                                    if !this.modem_connection_lost {
                                        this.terminal.lock().finish_modem_transfer();
                                    }
                                    this.modem_prompt_active = false;
                                    this.modem_connection_lost = false;
                                    this.modem_progress = None;
                                }
                                cx.notify();
                            })
                            .is_err()
                        {
                            break;
                        }
                        if done {
                            break;
                        }
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
                        let _ = weak.update(cx, |this, cx| {
                            if !this.modem_connection_lost {
                                this.terminal.lock().finish_modem_transfer();
                            }
                            this.modem_prompt_active = false;
                            this.modem_connection_lost = false;
                            this.modem_progress = None;
                            cx.notify();
                        });
                        break;
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
        self.terminal.lock().interrupt_modem_transfer();
        self.modem_progress = None;
        cx.notify();
    }

    fn notify_modem_connection_lost_if_active(&mut self) {
        if !self.modem_prompt_active || self.modem_connection_lost {
            return;
        }
        self.modem_connection_lost = true;
        self.terminal.lock().interrupt_modem_transfer();
        self.modem_progress = None;
        self.emit_trzsz_notice(
            self.preferences.trzsz_labels.connection_lost_title.clone(),
            None,
            TerminalNoticeVariant::Warning,
        );
    }
}
