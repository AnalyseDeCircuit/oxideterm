// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use super::*;

#[derive(Clone, Debug)]
pub(super) struct RemoteDesktopVncDownloadProgress {
    transfer_id: String,
    file_name: String,
    transferred_bytes: u64,
    total_bytes: u64,
    completed_files: u32,
    total_files: u32,
}

#[derive(Clone, Debug)]
pub(super) struct RemoteDesktopVncFileBrowserState {
    pub(super) open: bool,
    current_path: String,
    entries: Vec<RemoteDesktopRemoteFileEntry>,
    selected_paths: HashSet<String>,
    pub(super) pending_request_id: Option<String>,
    list_failed: bool,
    conflict_policy: RemoteDesktopFileConflictPolicy,
    pub(super) transfer: Option<RemoteDesktopVncDownloadProgress>,
}

impl Default for RemoteDesktopVncFileBrowserState {
    fn default() -> Self {
        Self {
            open: false,
            // An empty TightVNC list request asks the server for its roots,
            // avoiding any client-side assumption about Unix or DOS paths.
            current_path: String::new(),
            entries: Vec::new(),
            selected_paths: HashSet::new(),
            pending_request_id: None,
            list_failed: false,
            conflict_policy: RemoteDesktopFileConflictPolicy::Rename,
            transfer: None,
        }
    }
}

impl RemoteDesktopSessionEntity {
    pub(super) fn reset_vnc_file_browser_connection(&mut self) {
        self.vnc_files.open = false;
        self.vnc_files.current_path.clear();
        self.vnc_files.entries.clear();
        self.vnc_files.selected_paths.clear();
        self.vnc_files.pending_request_id = None;
        self.vnc_files.list_failed = false;
        self.vnc_files.transfer = None;
    }

    pub(super) fn vnc_file_download_available(&self) -> bool {
        let snapshot = self.state.snapshot();
        self.profile.protocol == RemoteDesktopProtocol::Vnc
            && self.profile.session_options.clipboard.files
            && snapshot.status == RemoteDesktopSessionStatus::Connected
            && snapshot
                .negotiated_capabilities
                .as_ref()
                .is_some_and(|capabilities| {
                    capabilities.vendor_file_list.is_supported()
                        && capabilities.vendor_file_download.is_supported()
                })
    }

    fn request_vnc_file_list(&mut self, path: String) {
        let request_id = uuid::Uuid::new_v4().to_string();
        self.vnc_files.pending_request_id = Some(request_id.clone());
        self.vnc_files.list_failed = false;
        self.send_request(RemoteDesktopHelperRequest::VncListRemoteFiles { request_id, path });
    }

    pub(super) fn apply_vnc_file_event(
        &mut self,
        event: RemoteDesktopHelperEvent,
    ) -> Option<RemoteDesktopDeliveryIntent> {
        match event {
            RemoteDesktopHelperEvent::VncRemoteFilesListed {
                request_id,
                path,
                mut entries,
            } if self.vnc_files.pending_request_id.as_deref() == Some(request_id.as_str()) => {
                entries.sort_by(|left, right| {
                    let left_directory = left.kind == RemoteDesktopRemoteFileKind::Directory;
                    let right_directory = right.kind == RemoteDesktopRemoteFileKind::Directory;
                    right_directory
                        .cmp(&left_directory)
                        .then_with(|| left.name.to_lowercase().cmp(&right.name.to_lowercase()))
                });
                self.vnc_files.current_path = path;
                self.vnc_files.entries = entries;
                self.vnc_files.pending_request_id = None;
                self.vnc_files.list_failed = false;
                None
            }
            RemoteDesktopHelperEvent::VncRemoteFileListFailed { request_id }
                if self.vnc_files.pending_request_id.as_deref() == Some(request_id.as_str()) =>
            {
                self.vnc_files.pending_request_id = None;
                self.vnc_files.list_failed = true;
                None
            }
            RemoteDesktopHelperEvent::VncFileTransferProgress {
                transfer_id,
                file_name,
                transferred_bytes,
                total_bytes,
                completed_files,
                total_files,
            } if self
                .vnc_files
                .transfer
                .as_ref()
                .is_some_and(|transfer| transfer.transfer_id == transfer_id) =>
            {
                self.vnc_files.transfer = Some(RemoteDesktopVncDownloadProgress {
                    transfer_id,
                    file_name,
                    transferred_bytes,
                    total_bytes,
                    completed_files,
                    total_files,
                });
                None
            }
            RemoteDesktopHelperEvent::VncFileTransferCompleted { transfer_id, .. }
                if self
                    .vnc_files
                    .transfer
                    .as_ref()
                    .is_some_and(|transfer| transfer.transfer_id == transfer_id) =>
            {
                self.vnc_files.transfer = None;
                self.vnc_files.selected_paths.clear();
                Some(RemoteDesktopDeliveryIntent::VncFileTransferCompleted)
            }
            RemoteDesktopHelperEvent::VncFileTransferFailed { transfer_id, kind }
                if self
                    .vnc_files
                    .transfer
                    .as_ref()
                    .is_some_and(|transfer| transfer.transfer_id == transfer_id) =>
            {
                self.vnc_files.transfer = None;
                Some(RemoteDesktopDeliveryIntent::VncFileTransferFailed(kind))
            }
            _ => None,
        }
    }
}

impl WorkspaceApp {
    pub(in crate::workspace) fn open_vnc_file_browser(
        &mut self,
        tab_id: TabId,
        cx: &mut Context<Self>,
    ) {
        let Some(session_entity) = self.remote_desktop_session_entity(tab_id, cx) else {
            return;
        };
        session_entity.update(cx, |session, cx| {
            if !session.vnc_file_download_available() {
                return;
            }
            session.vnc_files.open = true;
            if session.vnc_files.entries.is_empty()
                && session.vnc_files.pending_request_id.is_none()
            {
                session.request_vnc_file_list(session.vnc_files.current_path.clone());
            }
            cx.notify();
        });
    }

    fn close_vnc_file_browser(&mut self, tab_id: TabId, cx: &mut Context<Self>) {
        if let Some(session_entity) = self.remote_desktop_session_entity(tab_id, cx) {
            session_entity.update(cx, |session, cx| {
                session.vnc_files.open = false;
                cx.notify();
            });
        }
    }

    fn navigate_vnc_file_browser(&mut self, tab_id: TabId, path: String, cx: &mut Context<Self>) {
        if let Some(session_entity) = self.remote_desktop_session_entity(tab_id, cx) {
            session_entity.update(cx, |session, cx| {
                if session.vnc_files.pending_request_id.is_none()
                    && session.vnc_files.transfer.is_none()
                {
                    session.request_vnc_file_list(path);
                    cx.notify();
                }
            });
        }
    }

    fn toggle_vnc_file_selection(&mut self, tab_id: TabId, path: String, cx: &mut Context<Self>) {
        if let Some(session_entity) = self.remote_desktop_session_entity(tab_id, cx) {
            session_entity.update(cx, |session, cx| {
                if !session.vnc_files.selected_paths.remove(&path) {
                    session.vnc_files.selected_paths.insert(path);
                }
                cx.notify();
            });
        }
    }

    fn set_vnc_file_conflict_policy(
        &mut self,
        tab_id: TabId,
        policy: RemoteDesktopFileConflictPolicy,
        cx: &mut Context<Self>,
    ) {
        if let Some(session_entity) = self.remote_desktop_session_entity(tab_id, cx) {
            session_entity.update(cx, |session, cx| {
                session.vnc_files.conflict_policy = policy;
                cx.notify();
            });
        }
    }

    fn choose_vnc_download_destination(&mut self, tab_id: TabId, cx: &mut Context<Self>) {
        let Some(session_entity) = self.remote_desktop_session_entity(tab_id, cx) else {
            return;
        };
        let (remote_paths, conflict_policy) = {
            let session = session_entity.read(cx);
            if session.vnc_files.transfer.is_some() || session.vnc_files.selected_paths.is_empty() {
                return;
            }
            (
                session
                    .vnc_files
                    .selected_paths
                    .iter()
                    .cloned()
                    .collect::<Vec<_>>(),
                session.vnc_files.conflict_policy,
            )
        };
        let receiver = cx.prompt_for_paths(PathPromptOptions {
            files: false,
            directories: true,
            multiple: false,
            prompt: Some(SharedString::from(
                self.i18n.t("remote_desktop.file_download_destination"),
            )),
        });
        cx.spawn(async move |workspace, cx| {
            let destination = match receiver.await {
                Ok(Ok(Some(paths))) => paths.into_iter().next(),
                _ => None,
            };
            let Some(destination) = destination else {
                return;
            };
            let _ = workspace.update(cx, |workspace, cx| {
                let Some(session_entity) = workspace.remote_desktop_session_entity(tab_id, cx)
                else {
                    return;
                };
                session_entity.update(cx, |session, cx| {
                    if session.vnc_files.transfer.is_some() || !session.vnc_files.open {
                        return;
                    }
                    let transfer_id = uuid::Uuid::new_v4().to_string();
                    let total_bytes = session
                        .vnc_files
                        .entries
                        .iter()
                        .filter(|entry| remote_paths.contains(&entry.path))
                        .filter_map(|entry| entry.size)
                        .sum();
                    session.vnc_files.transfer = Some(RemoteDesktopVncDownloadProgress {
                        transfer_id: transfer_id.clone(),
                        file_name: String::new(),
                        transferred_bytes: 0,
                        total_bytes,
                        completed_files: 0,
                        total_files: u32::try_from(remote_paths.len()).unwrap_or(u32::MAX),
                    });
                    session.send_request(RemoteDesktopHelperRequest::VncDownloadRemoteFiles {
                        transfer_id,
                        remote_paths,
                        destination,
                        conflict_policy,
                    });
                    cx.notify();
                });
            });
        })
        .detach();
    }

    fn cancel_vnc_file_download(&mut self, tab_id: TabId, cx: &mut Context<Self>) {
        if let Some(session_entity) = self.remote_desktop_session_entity(tab_id, cx) {
            session_entity.update(cx, |session, cx| {
                let Some(transfer) = session.vnc_files.transfer.as_ref() else {
                    return;
                };
                session.send_request(RemoteDesktopHelperRequest::CancelVncFileTransfer {
                    transfer_id: transfer.transfer_id.clone(),
                });
                cx.notify();
            });
        }
    }

    pub(super) fn render_vnc_file_browser(
        &self,
        tab_id: TabId,
        browser: RemoteDesktopVncFileBrowserState,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let loading = browser.pending_request_id.is_some();
        let path_label = if browser.current_path.is_empty() {
            self.i18n.t("remote_desktop.file_browser_roots")
        } else {
            browser.current_path.clone()
        };
        let parent_path = remote_vnc_parent_path(&browser.current_path);
        let rows = if loading {
            vec![self.render_vnc_file_browser_message(
                self.i18n.t("remote_desktop.file_browser_loading"),
            )]
        } else if browser.list_failed {
            vec![
                self.render_vnc_file_browser_message(
                    self.i18n.t("remote_desktop.file_browser_failed"),
                ),
            ]
        } else if browser.entries.is_empty() {
            vec![
                self.render_vnc_file_browser_message(
                    self.i18n.t("remote_desktop.file_browser_empty"),
                ),
            ]
        } else {
            browser
                .entries
                .iter()
                .map(|entry| {
                    let entry_path = entry.path.clone();
                    let is_directory = entry.kind == RemoteDesktopRemoteFileKind::Directory;
                    let selected = browser.selected_paths.contains(&entry.path);
                    let icon = if is_directory {
                        LucideIcon::Folder
                    } else if selected {
                        LucideIcon::CheckSquare
                    } else {
                        LucideIcon::Square
                    };
                    let size = entry.size.map(format_remote_file_size).unwrap_or_default();
                    let modified = entry
                        .modified_seconds
                        .and_then(|seconds| i64::try_from(seconds).ok())
                        .and_then(|seconds| chrono::DateTime::from_timestamp(seconds, 0))
                        .map(|timestamp| {
                            timestamp
                                .with_timezone(&chrono::Local)
                                .format("%Y-%m-%d %H:%M")
                                .to_string()
                        })
                        .unwrap_or_default();
                    div()
                        .h(px(32.0))
                        .px(px(8.0))
                        .flex()
                        .items_center()
                        .gap(px(8.0))
                        .rounded(px(self.tokens.radii.sm))
                        .cursor_pointer()
                        .hover(|row| row.bg(rgb(theme.bg_hover)))
                        .child(Self::render_lucide_icon(
                            icon,
                            14.0,
                            rgb(if selected {
                                theme.accent
                            } else {
                                theme.text_muted
                            }),
                        ))
                        .child(
                            div()
                                .min_w(px(0.0))
                                .flex_1()
                                .truncate()
                                .text_size(px(self.tokens.metrics.ui_text_sm))
                                .text_color(rgb(theme.text))
                                .child(entry.name.clone()),
                        )
                        .child(
                            div()
                                .w(px(92.0))
                                .text_right()
                                .text_size(px(self.tokens.metrics.ui_text_xs))
                                .text_color(rgb(theme.text_muted))
                                .child(size),
                        )
                        .child(
                            div()
                                .w(px(116.0))
                                .text_right()
                                .text_size(px(self.tokens.metrics.ui_text_xs))
                                .text_color(rgb(theme.text_muted))
                                .child(modified),
                        )
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(move |this, _event, _window, cx| {
                                if is_directory {
                                    this.navigate_vnc_file_browser(tab_id, entry_path.clone(), cx);
                                } else {
                                    this.toggle_vnc_file_selection(tab_id, entry_path.clone(), cx);
                                }
                                cx.stop_propagation();
                            }),
                        )
                        .into_any_element()
                })
                .collect()
        };
        let progress = browser.transfer.as_ref().map(|transfer| {
            let percent = if transfer.total_bytes == 0 {
                0.0
            } else {
                (transfer.transferred_bytes as f32 / transfer.total_bytes as f32).clamp(0.0, 1.0)
            };
            let label = self
                .i18n
                .t("remote_desktop.file_download_progress")
                .replace("{{completed}}", &transfer.completed_files.to_string())
                .replace("{{total}}", &transfer.total_files.to_string())
                .replace("{{name}}", &transfer.file_name);
            div()
                .flex()
                .flex_col()
                .gap(px(5.0))
                .child(
                    div()
                        .text_size(px(self.tokens.metrics.ui_text_xs))
                        .text_color(rgb(theme.text_muted))
                        .child(label),
                )
                .child(
                    div()
                        .h(px(5.0))
                        .w_full()
                        .rounded_full()
                        .bg(rgb(theme.bg_hover))
                        .child(
                            div()
                                .h_full()
                                .w(relative(percent))
                                .rounded_full()
                                .bg(rgb(theme.accent)),
                        ),
                )
                .into_any_element()
        });
        let selected_count = browser.selected_paths.len();
        let selected_label = self
            .i18n
            .t("remote_desktop.file_browser_selected")
            .replace("{{count}}", &selected_count.to_string());

        dialog_overlay(
            &self.tokens,
            modal_container(&self.tokens)
                .w(px(620.0))
                .max_h(px(620.0))
                .flex()
                .flex_col()
                .on_mouse_down(MouseButton::Left, |_event, _window, cx| {
                    cx.stop_propagation();
                })
                .child(modal_header(
                    &self.tokens,
                    self.i18n.t("remote_desktop.file_browser_title"),
                    self.i18n.t("remote_desktop.file_browser_description"),
                ))
                .child(
                    modal_body(&self.tokens)
                        .flex_1()
                        .min_h(px(0.0))
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap(px(6.0))
                                .child(self.workspace_toolbar_action_button(
                                    self.i18n.t("remote_desktop.file_browser_up"),
                                    Some(Self::render_lucide_icon(
                                        LucideIcon::ArrowUp,
                                        12.0,
                                        rgb(theme.text_muted),
                                    )),
                                    compact_vnc_file_button(parent_path.is_none() || loading),
                                    cx.listener(move |this, _event, _window, cx| {
                                        if let Some(parent_path) = parent_path.clone() {
                                            this.navigate_vnc_file_browser(tab_id, parent_path, cx);
                                        }
                                    }),
                                ))
                                .child(self.workspace_toolbar_action_button(
                                    self.i18n.t("remote_desktop.file_browser_refresh"),
                                    Some(Self::render_lucide_icon(
                                        LucideIcon::RefreshCw,
                                        12.0,
                                        rgb(theme.text_muted),
                                    )),
                                    compact_vnc_file_button(loading),
                                    cx.listener(move |this, _event, _window, cx| {
                                        this.navigate_vnc_file_browser(
                                            tab_id,
                                            browser.current_path.clone(),
                                            cx,
                                        );
                                    }),
                                ))
                                .child(
                                    div()
                                        .min_w(px(0.0))
                                        .flex_1()
                                        .truncate()
                                        .text_size(px(self.tokens.metrics.ui_text_xs))
                                        .text_color(rgb(theme.text_muted))
                                        .child(path_label),
                                ),
                        )
                        .child(
                            div()
                                .h(px(320.0))
                                .overflow_y_scrollbar()
                                .border_1()
                                .border_color(rgb(theme.border))
                                .rounded(px(self.tokens.radii.sm))
                                .p(px(4.0))
                                .children(rows),
                        )
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap(px(6.0))
                                .child(
                                    div()
                                        .mr(px(4.0))
                                        .text_size(px(self.tokens.metrics.ui_text_xs))
                                        .text_color(rgb(theme.text_muted))
                                        .child(self.i18n.t("remote_desktop.file_conflict_policy")),
                                )
                                .children(
                                    [
                                        (
                                            RemoteDesktopFileConflictPolicy::Rename,
                                            self.i18n.t("remote_desktop.file_conflict_rename"),
                                        ),
                                        (
                                            RemoteDesktopFileConflictPolicy::Skip,
                                            self.i18n.t("remote_desktop.file_conflict_skip"),
                                        ),
                                        (
                                            RemoteDesktopFileConflictPolicy::Overwrite,
                                            self.i18n.t("remote_desktop.file_conflict_overwrite"),
                                        ),
                                    ]
                                    .into_iter()
                                    .map(|(policy, label)| {
                                        self.workspace_toolbar_action_button(
                                            label,
                                            None,
                                            ToolbarButtonOptions {
                                                button: ButtonOptions {
                                                    variant: if browser.conflict_policy == policy {
                                                        ButtonVariant::Default
                                                    } else {
                                                        ButtonVariant::Secondary
                                                    },
                                                    size: ButtonSize::Sm,
                                                    radius: ButtonRadius::Md,
                                                    disabled: browser.transfer.is_some(),
                                                },
                                                height: Some(24.0),
                                                padding_x: Some(8.0),
                                                font_size: Some(self.tokens.metrics.ui_text_xs),
                                                ..ToolbarButtonOptions::default()
                                            },
                                            cx.listener(move |this, _event, _window, cx| {
                                                this.set_vnc_file_conflict_policy(
                                                    tab_id, policy, cx,
                                                );
                                            }),
                                        )
                                    }),
                                ),
                        )
                        .when_some(progress, |body, progress| body.child(progress)),
                )
                .child(
                    modal_footer(&self.tokens)
                        .child(
                            div()
                                .mr_auto()
                                .text_size(px(self.tokens.metrics.ui_text_xs))
                                .text_color(rgb(theme.text_muted))
                                .child(selected_label),
                        )
                        .child(self.workspace_toolbar_action_button(
                            self.i18n.t("ssh.form.cancel"),
                            None,
                            compact_vnc_file_button(false),
                            cx.listener(move |this, _event, _window, cx| {
                                this.close_vnc_file_browser(tab_id, cx);
                            }),
                        ))
                        .when(browser.transfer.is_some(), |footer| {
                            footer.child(self.workspace_toolbar_action_button(
                                self.i18n.t("remote_desktop.file_download_cancel"),
                                None,
                                compact_vnc_file_button(false),
                                cx.listener(move |this, _event, _window, cx| {
                                    this.cancel_vnc_file_download(tab_id, cx);
                                }),
                            ))
                        })
                        .when(browser.transfer.is_none(), |footer| {
                            footer.child(self.workspace_toolbar_action_button(
                                self.i18n.t("remote_desktop.file_download"),
                                Some(Self::render_lucide_icon(
                                    LucideIcon::Download,
                                    12.0,
                                    rgb(theme.text_muted),
                                )),
                                ToolbarButtonOptions {
                                    button: ButtonOptions {
                                        variant: ButtonVariant::Default,
                                        size: ButtonSize::Sm,
                                        radius: ButtonRadius::Md,
                                        disabled: selected_count == 0,
                                    },
                                    height: Some(24.0),
                                    padding_x: Some(8.0),
                                    font_size: Some(self.tokens.metrics.ui_text_xs),
                                    ..ToolbarButtonOptions::default()
                                },
                                cx.listener(move |this, _event, _window, cx| {
                                    this.choose_vnc_download_destination(tab_id, cx);
                                }),
                            ))
                        }),
                ),
        )
    }

    fn render_vnc_file_browser_message(&self, message: String) -> AnyElement {
        div()
            .h(px(80.0))
            .flex()
            .items_center()
            .justify_center()
            .text_size(px(self.tokens.metrics.ui_text_sm))
            .text_color(rgb(self.tokens.ui.text_muted))
            .child(message)
            .into_any_element()
    }
}

fn compact_vnc_file_button(disabled: bool) -> ToolbarButtonOptions {
    ToolbarButtonOptions {
        button: ButtonOptions {
            variant: ButtonVariant::Secondary,
            size: ButtonSize::Sm,
            radius: ButtonRadius::Md,
            disabled,
        },
        height: Some(24.0),
        padding_x: Some(8.0),
        font_size: Some(12.0),
        ..ToolbarButtonOptions::default()
    }
}

fn remote_vnc_parent_path(path: &str) -> Option<String> {
    if path.is_empty() {
        return None;
    }
    if matches!(path, "/" | "\\") {
        return Some(String::new());
    }
    let trimmed = path.trim_end_matches(['/', '\\']);
    if trimmed.ends_with(':') {
        return Some(String::new());
    }
    let Some(separator) = trimmed.rfind(['/', '\\']) else {
        return Some(String::new());
    };
    if separator == 0 {
        Some("/".to_string())
    } else {
        Some(trimmed[..separator].to_string())
    }
}

fn format_remote_file_size(bytes: u64) -> String {
    const UNITS: [&str; 4] = ["B", "KiB", "MiB", "GiB"];
    let mut value = bytes as f64;
    let mut unit = 0usize;
    while value >= 1024.0 && unit + 1 < UNITS.len() {
        value /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{bytes} {}", UNITS[unit])
    } else {
        format!("{value:.1} {}", UNITS[unit])
    }
}
