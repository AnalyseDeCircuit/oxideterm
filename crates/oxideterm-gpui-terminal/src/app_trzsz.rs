impl TerminalPane {
    fn handle_trzsz_transfer_prompt(
        &mut self,
        request: TrzszPromptRequest,
        cx: &mut Context<Self>,
    ) {
        if self.trzsz_prompt_active {
            return;
        }
        // Match Tauri's controller boundary: once the magic key is accepted,
        // the protocol owner moves to a transfer worker while PTY output keeps
        // flowing into the same buffer through the terminal-side input handle.
        let Some(transfer) = self.terminal.lock().take_trzsz_transfer() else {
            return;
        };

        self.trzsz_prompt_active = true;
        self.trzsz_connection_lost = false;
        self.emit_trzsz_prompt_notice(&request);
        let receiver = match request.direction {
            TrzszTransferDirection::Upload => {
                let directory = request.selection == TrzszTransferSelection::Directory;
                cx.prompt_for_paths(PathPromptOptions {
                    files: !directory,
                    directories: directory,
                    multiple: true,
                    prompt: Some(SharedString::from(if directory {
                        self.preferences
                            .trzsz_labels
                            .select_upload_directory_title
                            .clone()
                    } else {
                        self.preferences
                            .trzsz_labels
                            .select_upload_files_title
                            .clone()
                    })),
                })
            }
            TrzszTransferDirection::Download => cx.prompt_for_paths(PathPromptOptions {
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

        let state = self.trzsz_state.clone();
        let owner_id = self.trzsz_owner_id.clone();
        let policy = self.preferences.trzsz_policy.clone().unwrap_or_default();
        let terminal_columns = self.snapshot.cols;
        cx.spawn(async move |weak, cx| {
            let selection = match receiver.await {
                Ok(Ok(Some(paths))) => match request.direction {
                    TrzszTransferDirection::Upload => TrzszPromptSelection::Upload(
                        paths
                            .into_iter()
                            .map(|path| path.to_string_lossy().to_string())
                            .collect(),
                    ),
                    TrzszTransferDirection::Download => paths
                        .into_iter()
                        .next()
                        .map(|path| {
                            TrzszPromptSelection::DownloadRoot(path.to_string_lossy().to_string())
                        })
                        .unwrap_or(TrzszPromptSelection::Cancelled),
                },
                _ => TrzszPromptSelection::Cancelled,
            };
            let (result_tx, result_rx) = std::sync::mpsc::channel();
            let (event_tx, event_rx) = std::sync::mpsc::channel();
            // The worker blocks on trzsz protocol reads, so it must never run
            // while holding the terminal session lock. The terminal tick keeps
            // draining PTY output and flushing worker writes back to SSH.
            std::thread::spawn(move || {
                let result = run_trzsz_worker_job(TrzszWorkerJob {
                    transfer,
                    request,
                    selection,
                    owner_id,
                    state,
                    policy,
                    event_tx,
                    terminal_columns,
                })
                .map_err(|error| error.to_string());
                let _ = result_tx.send(result);
            });

            loop {
                while let Ok(event) = event_rx.try_recv() {
                    let _ = weak.update(cx, |this, cx| {
                        this.handle_trzsz_worker_event(event, cx);
                    });
                }
                match result_rx.try_recv() {
                    Ok(result) => {
                        let _ = weak.update(cx, |this, cx| {
                            while let Ok(event) = event_rx.try_recv() {
                                this.handle_trzsz_worker_event(event, cx);
                            }
                            if !this.trzsz_connection_lost {
                                this.terminal.lock().finish_trzsz_transfer();
                            }
                            this.trzsz_prompt_active = false;
                            this.trzsz_connection_lost = false;
                            let _ = result;
                            cx.notify();
                        });
                        break;
                    }
                    Err(std::sync::mpsc::TryRecvError::Empty) => {
                        cx.background_executor()
                            .timer(Duration::from_millis(16))
                            .await;
                    }
                    Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                        let _ = weak.update(cx, |this, cx| {
                            while let Ok(event) = event_rx.try_recv() {
                                this.handle_trzsz_worker_event(event, cx);
                            }
                            if !this.trzsz_connection_lost {
                                this.terminal.lock().finish_trzsz_transfer();
                            }
                            this.trzsz_prompt_active = false;
                            this.trzsz_connection_lost = false;
                            cx.notify();
                        });
                        break;
                    }
                }
            }
        })
        .detach();
    }

    fn handle_trzsz_worker_event(&mut self, event: TrzszWorkerEvent, cx: &mut Context<Self>) {
        match event {
            TrzszWorkerEvent::TerminalOutput(bytes) => {
                // Tauri writes TextProgressBar VT output into the local terminal
                // renderer. Sending it back to the remote PTY would corrupt the
                // trzsz protocol stream, so native has a dedicated local feed.
                let snapshot = {
                    let mut terminal = self.terminal.lock();
                    terminal.feed_trzsz_terminal_output(&bytes);
                    terminal.snapshot()
                };
                self.snapshot = self.stamp_snapshot(snapshot);
                cx.notify();
            }
            TrzszWorkerEvent::Completed => {
                if self.trzsz_connection_lost {
                    return;
                }
                self.emit_trzsz_notice(
                    self.preferences.trzsz_labels.completed_title.clone(),
                    Some(self.preferences.trzsz_labels.completed_description.clone()),
                    TerminalNoticeVariant::Success,
                );
            }
            TrzszWorkerEvent::Cancelled => {
                if self.trzsz_connection_lost {
                    return;
                }
                self.emit_trzsz_notice(
                    self.preferences.trzsz_labels.cancelled_title.clone(),
                    Some(self.preferences.trzsz_labels.cancelled_description.clone()),
                    TerminalNoticeVariant::Warning,
                );
            }
            TrzszWorkerEvent::PartialCleanup => {
                self.emit_trzsz_notice(
                    self.preferences.trzsz_labels.partial_cleanup_title.clone(),
                    Some(
                        self.preferences
                            .trzsz_labels
                            .partial_cleanup_description
                            .clone(),
                    ),
                    TerminalNoticeVariant::Warning,
                );
            }
            TrzszWorkerEvent::Failed {
                code,
                detail,
                message,
                ..
            } => {
                if self.trzsz_connection_lost {
                    return;
                }
                let (title, description, variant) =
                    self.trzsz_failure_notice(&code, detail.as_deref(), &message);
                self.emit_trzsz_notice(title, Some(description), variant);
            }
        }
    }

    fn notify_trzsz_connection_lost_if_active(&mut self) {
        if !self.trzsz_prompt_active || self.trzsz_connection_lost {
            return;
        }

        self.trzsz_connection_lost = true;
        // Mirrors TerminalView.disposeTrzszController({ notifyConnectionLost: true }):
        // emit one connection-lost toast, then stop the protocol buffer so the
        // transfer worker is unblocked instead of waiting for more PTY data.
        self.terminal.lock().interrupt_trzsz_transfer();
        self.emit_trzsz_notice(
            self.preferences.trzsz_labels.connection_lost_title.clone(),
            Some(
                self.preferences
                    .trzsz_labels
                    .connection_lost_description
                    .clone(),
            ),
            TerminalNoticeVariant::Warning,
        );
    }

    fn emit_trzsz_prompt_notice(&self, request: &TrzszPromptRequest) {
        let labels = &self.preferences.trzsz_labels;
        let (title, description) = match request.direction {
            TrzszTransferDirection::Upload
                if request.selection == TrzszTransferSelection::Directory =>
            {
                (
                    labels.select_upload_directory_title.clone(),
                    labels.select_upload_directory_description.clone(),
                )
            }
            TrzszTransferDirection::Upload => (
                labels.select_upload_files_title.clone(),
                labels.select_upload_files_description.clone(),
            ),
            TrzszTransferDirection::Download => (
                labels.select_download_directory_title.clone(),
                labels.select_download_directory_description.clone(),
            ),
        };
        self.emit_trzsz_notice(title, Some(description), TerminalNoticeVariant::Default);
    }

    fn emit_trzsz_notice(
        &self,
        title: String,
        description: Option<String>,
        variant: TerminalNoticeVariant,
    ) {
        if let Some(sink) = &self.preferences.notice_sink {
            sink(TerminalNotice {
                title,
                description,
                status_text: None,
                progress: None,
                variant,
            });
        }
    }

    fn trzsz_failure_notice(
        &self,
        code: &str,
        detail: Option<&str>,
        fallback: &str,
    ) -> (String, String, TerminalNoticeVariant) {
        let labels = &self.preferences.trzsz_labels;
        match code {
            "invalid_api_version" | "root_mismatch" | "root_not_prepared" => (
                labels.version_mismatch_title.clone(),
                labels.version_mismatch_description.clone(),
                TerminalNoticeVariant::Error,
            ),
            "invalid_path" | "unauthorized_path" | "reserved_name" => (
                labels.path_invalid_title.clone(),
                labels.path_invalid_description.clone(),
                TerminalNoticeVariant::Error,
            ),
            "symlink_not_allowed" => (
                labels.symlink_not_supported_title.clone(),
                labels.symlink_not_supported_description.clone(),
                TerminalNoticeVariant::Error,
            ),
            "already_exists" => (
                labels.conflict_detected_title.clone(),
                labels.conflict_detected_description.clone(),
                TerminalNoticeVariant::Warning,
            ),
            "directory_not_allowed" => (
                labels.directory_not_allowed_title.clone(),
                labels.directory_not_allowed_description.clone(),
                TerminalNoticeVariant::Warning,
            ),
            "max_file_count_exceeded" => (
                labels.max_file_count_title.clone(),
                format_count_limit_message(&labels.max_file_count_description, detail),
                TerminalNoticeVariant::Warning,
            ),
            "max_total_bytes_exceeded" => (
                labels.max_total_bytes_title.clone(),
                format_byte_limit_message(&labels.max_total_bytes_description, detail),
                TerminalNoticeVariant::Warning,
            ),
            _ => (
                labels.failed_title.clone(),
                if fallback.is_empty() {
                    labels.failed_description.clone()
                } else {
                    fallback.to_string()
                },
                TerminalNoticeVariant::Error,
            ),
        }
    }

}

fn format_count_limit_message(template: &str, detail: Option<&str>) -> String {
    let selected = detail_value(detail, "selected").unwrap_or_else(|| "0".to_string());
    let max = detail_value(detail, "max").unwrap_or_else(|| "0".to_string());
    template
        .replace("{{selected}}", &selected)
        .replace("{{max}}", &max)
}

fn format_byte_limit_message(template: &str, detail: Option<&str>) -> String {
    let selected = detail_value(detail, "selected")
        .and_then(|value| value.parse::<u64>().ok())
        .map(format_binary_size)
        .unwrap_or_else(|| "0 B".to_string());
    let max = detail_value(detail, "max")
        .and_then(|value| value.parse::<u64>().ok())
        .map(format_binary_size)
        .unwrap_or_else(|| "0 B".to_string());
    template
        .replace("{{selected}}", &selected)
        .replace("{{max}}", &max)
}

fn detail_value(detail: Option<&str>, key: &str) -> Option<String> {
    detail?
        .split(',')
        .filter_map(|part| part.trim().split_once('='))
        .find_map(|(name, value)| (name == key).then(|| value.trim().to_string()))
}

fn format_binary_size(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit < UNITS.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{bytes} B")
    } else if value >= 100.0 {
        format!("{value:.0} {}", UNITS[unit])
    } else if value >= 10.0 {
        format!("{value:.1} {}", UNITS[unit])
    } else {
        format!("{value:.2} {}", UNITS[unit])
    }
}
