//! Connects filesystem view actions to the shared Host Tools entity.

use super::*;
use oxideterm_connection_monitor::parse_filesystem_snapshot;

impl HostToolsEntity {
    pub fn render_filesystems_panel(
        &self,
        search: FilesystemSearchFrame,
        sidebar_width: f32,
        tokens: &ThemeTokens,
        i18n: &I18n,
        mono_font_family: SharedString,
        selectable_text: &HostToolsText,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let connections = self.monitor_connections();
        let Some(first) = connections.first() else {
            return host_tools_center_state(
                "lucide/wifi-off.svg",
                tokens.ui.text_muted,
                i18n.t("profiler.panel.no_connection"),
                selectable_text,
                cx,
            );
        };
        let selected_id = self
            .selected_connection_id()
            .unwrap_or(first.connection_id.as_str());
        let connection_switcher = self.render_connection_switcher(
            &connections,
            selected_id,
            !self.host_filesystems.refresh_in_flight(),
            tokens,
            mono_font_family.clone(),
            selectable_text,
            cx,
        );
        let owner = cx.entity().downgrade();
        let action_i18n = i18n.clone();
        FilesystemPanelView {
            state: &self.host_filesystems,
            connection_id: selected_id,
            search,
            sidebar_width,
            tokens,
            i18n,
            mono_font_family,
            render_status: &|icon, color, label, cx| {
                let icon = match icon {
                    FilesystemStatusIcon::Drive => "lucide/hard-drive.svg",
                    FilesystemStatusIcon::Warning => "lucide/alert-triangle.svg",
                };
                host_tools_center_state(icon, color, label, selectable_text, cx)
            },
            on_action: std::rc::Rc::new(move |action, window, cx| {
                let _ = owner.update(cx, |host_tools, cx| {
                    host_tools.handle_filesystem_action(action, &action_i18n, window, cx);
                });
            }),
        }
        .render(connection_switcher, cx)
    }

    fn handle_filesystem_action(
        &mut self,
        action: FilesystemAction,
        i18n: &I18n,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match action {
            FilesystemAction::FocusSearch(event) => {
                let input = HostToolsTextInput::FilesystemSearch;
                self.ui.focus_input(input);
                // The shared window input handler resolves selection after this update.
                window.dispatch_action(
                    Box::new(HostToolsWindowRequest::new(
                        HostToolsWindowIntent::BeginPlainTextImeSelection { input, event },
                    )),
                    cx,
                );
            }
            FilesystemAction::SelectFilter(filter) => {
                if self.host_filesystems.select_filter(filter) {
                    cx.notify();
                }
            }
            FilesystemAction::ToggleRow(index) => {
                self.host_filesystems.toggle_expanded(index);
                cx.notify();
            }
            FilesystemAction::Refresh => {
                self.request_active_tool_snapshot(HostSnapshotFeedback::Toast, cx)
            }
            FilesystemAction::CopyPath(path) => self.copy_host_filesystem_path(path, cx),
            FilesystemAction::OpenDiagnostic(connection_id) => {
                self.dispatch_host_filesystem_diagnostic(connection_id, i18n, window, cx);
            }
        }
    }

    fn copy_host_filesystem_path(&mut self, path: String, cx: &mut Context<Self>) {
        cx.write_to_clipboard(ClipboardItem::new_string(path.clone()));
        cx.emit(HostToolsEvent::ShowNotice(
            HostToolsNotice::FilesystemPathCopied { path },
        ));
        cx.notify();
    }

    fn dispatch_host_filesystem_diagnostic(
        &self,
        connection_id: String,
        i18n: &I18n,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // Only the fixed OS-specific builder output may become a terminal command.
        let command = self.filesystem_diagnostic_command(&connection_id);
        window.dispatch_action(
            Box::new(HostToolsWindowRequest::new(
                HostToolsWindowIntent::OpenExistingNodeTerminal {
                    connection_id,
                    command,
                    title: i18n.t("sidebar.host_filesystems.diagnostic_title"),
                    opened_notice: i18n.t("sidebar.host_filesystems.toast.diagnostic_opened"),
                    missing_notice: i18n.t("sidebar.host_filesystems.toast.exec_terminal_missing"),
                },
            )),
            cx,
        );
    }

    pub(super) fn filesystem_diagnostic_command(&self, connection_id: &str) -> String {
        let os_type = self
            .connection_os_type(connection_id)
            .unwrap_or_else(|| "Unknown".to_string());
        build_filesystem_diagnostic_command(&os_type)
    }

    pub(crate) fn request_filesystem_snapshot(
        &mut self,
        connection_id: String,
        feedback: HostSnapshotFeedback,
        monitoring_enabled: bool,
        runtime: HostToolsRuntime,
        failure_fallback: String,
        cx: &mut Context<Self>,
    ) -> Vec<HostToolsNotice> {
        if !monitoring_enabled {
            return Vec::new();
        }
        if self.host_filesystems.refresh_in_flight() {
            return feedback
                .should_toast()
                .then_some(HostToolsNotice::FilesystemSnapshotAlreadyRunning)
                .into_iter()
                .collect();
        }
        let Some(os_type) = self.connection_os_type(&connection_id) else {
            return feedback
                .should_toast()
                .then_some(HostToolsNotice::FilesystemConnectionMissing)
                .into_iter()
                .collect();
        };
        let command = build_filesystem_snapshot_command(&os_type);
        let mut notices = Vec::new();
        if feedback.should_toast() && command.capability == FilesystemCommandCapability::Partial {
            notices.push(HostToolsNotice::FilesystemPartialSupport { os_type });
        }

        let request = FilesystemSnapshotRequest {
            connection_id,
            notify: feedback.should_toast(),
            failure_fallback,
        };
        self.host_filesystems.begin_refresh(request.clone());
        // Filesystem scans may touch du/find. Only the existing explicit
        // requests reach this path; the panel does not schedule periodic work.
        let spawned = self.spawn_filesystem_snapshot_capture(
            command.command,
            request.clone(),
            HOST_FILESYSTEM_SNAPSHOT_TIMEOUT,
            HOST_FILESYSTEM_SNAPSHOT_MAX_OUTPUT_SIZE,
            runtime,
        );
        if !spawned {
            self.host_filesystems.take_refresh(&request);
            return feedback
                .should_toast()
                .then_some(HostToolsNotice::FilesystemConnectionMissing)
                .into_iter()
                .collect();
        }
        cx.notify();
        notices
    }

    pub(crate) fn finish_host_filesystems_snapshot(
        &mut self,
        mut delivery: HostFilesystemSnapshotDelivery,
        cx: &mut Context<Self>,
    ) {
        if !self.host_filesystems.take_refresh(&delivery.request) {
            if let Ok(output) = delivery.result.as_mut() {
                zeroize_host_snapshot_output(output);
            }
            return;
        }

        let request = delivery.request;
        let failure_status = || ResourceFilesystemStatus::Error {
            message: request.failure_fallback.clone(),
        };
        let failure_snapshot = || ResourceFilesystemSnapshot {
            status: failure_status(),
            entries: Vec::new(),
        };
        let snapshot = match delivery.result {
            Ok(mut output) => {
                let snapshot = if output.exit_code.unwrap_or(0) == 0 {
                    let mut snapshot = parse_filesystem_snapshot(&output.stdout);
                    if matches!(snapshot.status, ResourceFilesystemStatus::Error { .. }) {
                        snapshot.status = failure_status();
                    }
                    snapshot
                } else {
                    failure_snapshot()
                };
                zeroize_host_snapshot_output(&mut output);
                snapshot
            }
            Err(()) => failure_snapshot(),
        };
        if request.notify {
            let notice = match &snapshot.status {
                ResourceFilesystemStatus::Available { .. } => {
                    Some(HostToolsNotice::FilesystemSnapshotLoaded {
                        count: snapshot.entries.len(),
                    })
                }
                ResourceFilesystemStatus::Unavailable => {
                    Some(HostToolsNotice::FilesystemUnavailable)
                }
                ResourceFilesystemStatus::Error { .. } => {
                    Some(HostToolsNotice::FilesystemSnapshotFailed)
                }
                ResourceFilesystemStatus::Unknown => None,
            };
            if let Some(notice) = notice {
                cx.emit(HostToolsEvent::ShowNotice(notice));
            }
        }
        self.host_filesystems.store_snapshot(request, snapshot);
        cx.notify();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use oxideterm_connection_monitor::FilesystemFilter;

    #[gpui::test]
    fn filesystem_filter_and_row_expansion_are_handled_by_the_panel(cx: &mut gpui::TestAppContext) {
        let (sender, receiver) = tokio::sync::mpsc::channel(1);
        let tools = cx
            .new(|cx| HostToolsEntity::new(sender, receiver, SshConnectionRegistry::default(), cx));
        let cx = cx.add_empty_window();
        cx.update(|window, cx| {
            tools.update(cx, |tools, cx| {
                let request = FilesystemSnapshotRequest {
                    connection_id: "fixture".into(),
                    notify: false,
                    failure_fallback: "Capture failed".into(),
                };
                let snapshot = parse_filesystem_snapshot(concat!(
                    "===FILESYSTEMS===\n",
                    "__OXIDE_FILESYSTEM_CAPABILITY__\tfull\tlinux_df\n",
                    "DF\t/dev/sda1\text4\t2000\t1000\t1000\t50\t/\n",
                    "DF\t/dev/sdb1\txfs\t2000\t1000\t1000\t50\t/archive\n",
                    "FINDMNT\tTARGET=\"/archive\" SOURCE=\"/dev/sdb1\" FSTYPE=\"xfs\" OPTIONS=\"ro\"\n",
                    "===FILESYSTEMS_END===\n",
                ));
                tools.host_filesystems.store_snapshot(request, snapshot);
                let i18n = I18n::new(oxideterm_i18n::Locale::En);
                tools.handle_filesystem_action(FilesystemAction::ToggleRow(0), &i18n, window, cx);
                assert_eq!(tools.host_filesystems.expanded_index(), Some(0));

                tools.handle_filesystem_action(
                    FilesystemAction::SelectFilter(FilesystemFilter::ReadOnly),
                    &i18n,
                    window,
                    cx,
                );
                assert_eq!(tools.host_filesystems.expanded_index(), None);
                let paths = tools.host_filesystems.visible_rows("fixture")
                    .into_iter().map(|row| row.path).collect::<Vec<_>>();
                assert_eq!(paths, ["/archive"]);
            });
        });
    }
}
