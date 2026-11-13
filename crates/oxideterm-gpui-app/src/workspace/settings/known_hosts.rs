use super::*;
use gpui::{ClipboardItem, FontWeight, ScrollHandle, Task};
use oxideterm_ssh::{KnownHostEntry, KnownHostsSnapshot, list_known_hosts, remove_host_key};

#[derive(Default)]
pub(super) struct KnownHostsState {
    pub snapshot: Option<KnownHostsSnapshot>,
    pub error: Option<String>,
    pub query: String,
    pub pending_delete: Option<KnownHostEntry>,
    pub task: Option<Task<()>>,
    pub scroll: ScrollHandle,
}

impl SettingsWorkspaceEntity {
    pub(super) fn refresh_known_hosts(&mut self, cx: &mut Context<Self>) {
        if self.known_hosts.task.is_some() {
            return;
        }
        self.known_hosts.pending_delete = None;
        self.known_hosts.error = None;
        let read = cx
            .background_executor()
            .spawn(async { list_known_hosts().map_err(|error| error.to_string()) });
        self.known_hosts.task = Some(cx.spawn(async move |settings, cx| {
            let result = read.await;
            let _ = settings.update(cx, |settings, cx| {
                settings.known_hosts.task = None;
                match result {
                    Ok(snapshot) => settings.known_hosts.snapshot = Some(snapshot),
                    Err(error) => settings.known_hosts.error = Some(error),
                }
                cx.notify();
            });
        }));
        cx.notify();
    }

    fn delete_known_host(&mut self, cx: &mut Context<Self>) {
        if self.known_hosts.task.is_some() {
            return;
        }
        let Some(entry) = self
            .known_hosts
            .pending_delete
            .take()
            .filter(|entry| entry.can_remove)
        else {
            return;
        };
        let Some(port) = entry.port else {
            return;
        };
        self.known_hosts.error = None;
        // The captured identity is the row confirmed by the user, not its current list index.
        let remove = cx.background_executor().spawn(async move {
            remove_host_key(&entry.host, port, &entry.key_type, &entry.fingerprint)
                .and_then(|()| list_known_hosts())
                .map_err(|error| error.to_string())
        });
        self.known_hosts.task = Some(cx.spawn(async move |settings, cx| {
            let result = remove.await;
            let _ = settings.update(cx, |settings, cx| {
                settings.known_hosts.task = None;
                match result {
                    Ok(snapshot) => settings.known_hosts.snapshot = Some(snapshot),
                    Err(error) => settings.known_hosts.error = Some(error),
                }
                cx.notify();
            });
        }));
        cx.notify();
    }
}

impl WorkspaceApp {
    fn known_hosts_button(
        &self,
        label: &'static str,
        icon: LucideIcon,
        danger: bool,
        disabled: bool,
        cx: &mut Context<Self>,
        action: impl Fn(&mut Self, &mut Window, &mut Context<Self>) + 'static,
    ) -> AnyElement {
        self.workspace_toolbar_action_button(
            self.i18n.t(label),
            Some(Self::render_lucide_icon(
                icon,
                14.0,
                rgb(self.tokens.ui.text),
            )),
            ToolbarButtonOptions {
                button: ButtonOptions {
                    variant: if danger {
                        ButtonVariant::Destructive
                    } else {
                        ButtonVariant::Outline
                    },
                    size: ButtonSize::Sm,
                    radius: ButtonRadius::Md,
                    disabled,
                },
                font_size: Some(self.tokens.metrics.ui_text_sm),
                ..ToolbarButtonOptions::default()
            },
            cx.listener(move |this, _, window, cx| {
                if !disabled {
                    action(this, window, cx);
                }
                cx.stop_propagation();
            }),
        )
        .debug_selector(|| label.to_string())
        .into_any_element()
    }

    pub(super) fn settings_known_hosts_section(&self, cx: &mut Context<Self>) -> AnyElement {
        let state = &self.settings_workspace.read(cx).known_hosts;
        let busy = state.task.is_some();
        let query = state.query.to_lowercase();
        let entries: Vec<_> = state
            .snapshot
            .as_ref()
            .map(|snapshot| {
                snapshot
                    .entries
                    .iter()
                    .filter(|entry| {
                        query.is_empty()
                            || entry.host.to_lowercase().contains(&query)
                            || entry.fingerprint.to_lowercase().contains(&query)
                            || entry.key_type.to_lowercase().contains(&query)
                            || entry
                                .port
                                .is_some_and(|port| port.to_string().contains(&query))
                    })
                    .cloned()
                    .collect()
            })
            .unwrap_or_default();
        let pending_delete = state.pending_delete.clone();
        let scroll = state.scroll.clone();
        let file = state
            .snapshot
            .as_ref()
            .map(|snapshot| (snapshot.path.clone(), snapshot.exists));
        let error = state.error.clone();
        let has_error = error.is_some();
        let query_value = state.query.clone();
        let mut toolbar =
            div()
                .w_full()
                .min_w_0()
                .flex()
                .flex_wrap()
                .items_center()
                .gap(px(8.0))
                .child(div().flex_1().min_w(px(180.0)).child(
                    self.settings_text_input_control_fill(
                        SettingsInput::KnownHostsSearch,
                        query_value,
                        self.i18n.t("settings_view.known_hosts.search"),
                        cx,
                    ),
                ))
                .child(self.known_hosts_button(
                    "settings_view.known_hosts.refresh",
                    LucideIcon::RefreshCw,
                    false,
                    busy,
                    cx,
                    |this, _, cx| {
                        this.settings_workspace
                            .update(cx, |settings, cx| settings.refresh_known_hosts(cx));
                    },
                ));
        if let Some((path, exists)) = &file {
            let path = path.clone();
            toolbar = toolbar.child(self.known_hosts_button(
                "settings_view.known_hosts.open_file",
                LucideIcon::ExternalLink,
                false,
                !exists,
                cx,
                move |_, _, cx| {
                    if let Ok(url) = url::Url::from_file_path(&path) {
                        cx.open_url(url.as_str());
                    }
                },
            ));
        }
        let mut body = div()
            .w_full()
            .min_w_0()
            .flex()
            .flex_col()
            .gap(px(12.0))
            .child(toolbar);
        if let Some((path, _)) = file {
            body = body.child(
                div()
                    .text_size(px(self.tokens.metrics.ui_text_sm))
                    .text_color(rgb(self.tokens.ui.text_muted))
                    .child(
                        self.i18n
                            .t("settings_view.known_hosts.source")
                            .replace("{{path}}", &path.display().to_string()),
                    ),
            );
        }
        if let Some(error) = error {
            body = body.child(
                self.connection_status_row(
                    self.i18n
                        .t("settings_view.known_hosts.error")
                        .replace("{{error}}", &error),
                ),
            );
        }
        if busy {
            body = body.child(
                self.connection_status_row(self.i18n.t("settings_view.known_hosts.loading")),
            );
        }
        if entries.is_empty() && !busy && !has_error {
            body = body.child(self.connection_status_row(self.i18n.t(if query.is_empty() {
                "settings_view.known_hosts.empty"
            } else {
                "settings_view.known_hosts.no_matches"
            })));
        }
        let mut list = div().w_full().min_w_0().flex().flex_col();
        for (index, entry) in entries.into_iter().enumerate() {
            if index != 0 {
                list = list.child(self.card_separator());
            }
            let endpoint = match entry.port {
                Some(22) | None => entry.host.clone(),
                Some(port) => format!("[{}]:{port}", entry.host),
            };
            let fingerprint = entry.fingerprint.clone();
            let mut row = div()
                .id(("known-host-row", index))
                .debug_selector(|| format!("known-host-{endpoint}"))
                .w_full()
                .min_w_0()
                .py(px(12.0))
                .flex()
                .flex_col()
                .gap(px(8.0))
                .child(
                    div()
                        .min_w_0()
                        .text_size(px(self.tokens.metrics.ui_text_sm))
                        .font_weight(FontWeight::SEMIBOLD)
                        .child(endpoint),
                )
                .child(
                    div()
                        .min_w_0()
                        .text_size(px(self.tokens.metrics.ui_text_sm))
                        .child(format!("{} · {}", entry.key_type, entry.fingerprint)),
                );
            let mut actions = div().flex().flex_wrap().items_center().gap(px(8.0)).child(
                self.known_hosts_button(
                    "settings_view.known_hosts.copy",
                    LucideIcon::Copy,
                    false,
                    false,
                    cx,
                    move |_, _, cx| {
                        cx.write_to_clipboard(ClipboardItem::new_string(fingerprint.clone()));
                    },
                ),
            );
            if pending_delete.as_ref() == Some(&entry) {
                row = row.child(
                    div()
                        .text_size(px(self.tokens.metrics.ui_text_sm))
                        .text_color(rgb(self.tokens.ui.warning))
                        .child(self.i18n.t("settings_view.known_hosts.confirm_delete")),
                );
                actions = actions
                    .child(self.known_hosts_button(
                        "settings_view.known_hosts.cancel",
                        LucideIcon::X,
                        false,
                        busy,
                        cx,
                        |this, _, cx| {
                            this.settings_workspace.update(cx, |settings, cx| {
                                settings.known_hosts.pending_delete = None;
                                cx.notify();
                            });
                        },
                    ))
                    .child(self.known_hosts_button(
                        "settings_view.known_hosts.confirm",
                        LucideIcon::Trash2,
                        true,
                        busy,
                        cx,
                        |this, _, cx| {
                            this.settings_workspace
                                .update(cx, |settings, cx| settings.delete_known_host(cx));
                        },
                    ));
            } else if entry.can_remove {
                actions = actions.child(self.known_hosts_button(
                    "settings_view.known_hosts.delete",
                    LucideIcon::Trash2,
                    false,
                    busy,
                    cx,
                    move |this, _, cx| {
                        this.settings_workspace.update(cx, |settings, cx| {
                            settings.known_hosts.pending_delete = Some(entry.clone());
                            cx.notify();
                        });
                    },
                ));
            } else {
                actions = actions.child(
                    div()
                        .text_size(px(self.tokens.metrics.ui_text_sm))
                        .text_color(rgb(self.tokens.ui.text_muted))
                        .child(self.i18n.t("settings_view.known_hosts.read_only")),
                );
            }
            list = list.child(row.child(actions));
        }
        body = body.child(
            div()
                .id("known-hosts-list")
                .relative()
                .w_full()
                .max_h(px(360.0))
                .child(
                    div()
                        .id("known-hosts-scroll")
                        .w_full()
                        .max_h(px(360.0))
                        .selectable_overflow_y_scroll(&scroll)
                        .on_scroll(|_, _, cx| cx.stop_propagation())
                        .child(list),
                )
                .vertical_scrollbar(&scroll),
        );
        self.settings_card(
            "settings_view.known_hosts.title",
            "settings_view.known_hosts.description",
            vec![body.into_any_element()],
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::workspace::window_shell::WorkspaceWindowShell;
    use gpui::TestAppContext;

    #[gpui::test]
    fn known_hosts_search_and_confirmed_delete_use_the_selected_endpoint(cx: &mut TestAppContext) {
        let executable = std::env::current_exe().unwrap();
        let fixture_key = "OXIDETERM_KNOWN_HOSTS_UI_TEST_DIR";
        let Some(fixture_dir) = std::env::var_os(fixture_key) else {
            let directory = tempfile::tempdir_in(executable.parent().unwrap()).unwrap();
            let child = directory.path().join(executable.file_name().unwrap());
            std::fs::hard_link(&executable, &child).unwrap();
            std::fs::write(directory.path().join("portable"), []).unwrap();
            let output = std::process::Command::new(child)
                .args([cx.test_function_name().unwrap(), "--nocapture"])
                .env(fixture_key, directory.path())
                .env_remove("APPIMAGE")
                .current_dir(directory.path())
                .output()
                .unwrap();
            assert!(
                output.status.success(),
                "{}\n{}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            );
            return;
        };
        let path = list_known_hosts().unwrap().path;
        assert!(path.starts_with(PathBuf::from(fixture_dir)));
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        let key = "AAAAC3NzaC1lZDI1NTE5AAAAIJdD7y3aLq454yWBdwLWbieU1ebz9/cu7/QEXn9OIeZJ";
        let retained = format!("# shared trust\nboard,alias ssh-ed25519 {key}\n");
        let original = format!("{retained}[board]:2222 ssh-ed25519 {key}\n");
        std::fs::write(&path, &original).unwrap();
        let mut store = SettingsStore::load_default().unwrap();
        store.settings_mut().onboarding_completed = true;
        store.settings_mut().onboarding_disclaimer_accepted = true;
        store.settings_mut().ssh_config.auto_load_hosts = false;
        store.save().unwrap();
        crate::migration_snapshot::acknowledge_pre_2_0_migration_notice(&default_settings_path())
            .unwrap();
        cx.executor().allow_parking();
        let (shell, cx) = cx.add_window_view(|window, cx| {
            let workspace = cx.new(|cx| WorkspaceApp::new(window, cx, None, None).unwrap());
            WorkspaceWindowShell::new(workspace, window, cx)
        });
        cx.simulate_resize(gpui::size(px(1400.0), px(1000.0)));
        let workspace = shell.read_with(cx, |shell, _| shell.session_entity());
        workspace.update_in(cx, |this, window, cx| {
            this.settings_workspace.update(cx, |settings, cx| {
                settings.set_active_tab(SettingsTab::Connections, cx);
            });
            this.open_settings(window, cx);
            this.sync_settings_section_list_state(cx);
            this.settings_section_list_state
                .scroll_to(gpui::ListOffset {
                    item_ix: SETTINGS_SECTION_HEADER_ITEM_COUNT + 6,
                    offset_in_item: px(0.0),
                });
        });
        cx.run_until_parked();
        cx.update(|window, cx| window.draw(cx).clear(cx));
        let bounds = workspace.read_with(cx, |this, _| {
            this.text_input_anchors
                .get(WorkspaceImeTarget::Settings(SettingsInput::KnownHostsSearch).anchor_id())
                .unwrap()
                .bounds
        });
        cx.simulate_click(bounds.center(), gpui::Modifiers::default());
        cx.simulate_input("2222");
        workspace.read_with(cx, |this, cx| {
            assert_eq!(this.settings_workspace.read(cx).known_hosts.query, "2222");
        });
        cx.update(|window, cx| {
            workspace.update(cx, |this, cx| {
                this.sync_settings_section_list_state(cx);
                this.settings_section_list_state.scroll_to_end();
            });
            window.draw(cx).clear(cx);
        });
        assert!(cx.debug_bounds("known-host-board").is_none());
        assert!(cx.debug_bounds("known-host-[board]:2222").is_some());
        let delete = cx.debug_bounds("settings_view.known_hosts.delete").unwrap();
        assert!(
            delete.origin.y >= px(0.0) && delete.bottom() <= px(1000.0),
            "delete bounds: {delete:?}"
        );
        cx.simulate_click(delete.center(), gpui::Modifiers::default());
        workspace.read_with(cx, |this, cx| {
            let entry = this
                .settings_workspace
                .read(cx)
                .known_hosts
                .pending_delete
                .as_ref()
                .expect("clicking the visible delete button must request confirmation");
            assert_eq!((entry.host.as_str(), entry.port), ("board", Some(2222)));
        });
        assert_eq!(std::fs::read_to_string(&path).unwrap(), original);
        cx.update(|window, cx| window.draw(cx).clear(cx));
        let cancel = cx.debug_bounds("settings_view.known_hosts.cancel").unwrap();
        cx.simulate_click(cancel.center(), gpui::Modifiers::default());
        assert_eq!(std::fs::read_to_string(&path).unwrap(), original);
        cx.update(|window, cx| window.draw(cx).clear(cx));
        let delete = cx.debug_bounds("settings_view.known_hosts.delete").unwrap();
        cx.simulate_click(delete.center(), gpui::Modifiers::default());
        cx.update(|window, cx| window.draw(cx).clear(cx));
        let confirm = cx
            .debug_bounds("settings_view.known_hosts.confirm")
            .unwrap();
        cx.simulate_click(confirm.center(), gpui::Modifiers::default());
        cx.run_until_parked();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), retained);
        workspace.read_with(cx, |this, cx| {
            let state = &this.settings_workspace.read(cx).known_hosts;
            assert_eq!(state.error, None);
            assert_eq!(
                state
                    .snapshot
                    .as_ref()
                    .unwrap()
                    .entries
                    .iter()
                    .map(|entry| (entry.host.as_str(), entry.port))
                    .collect::<Vec<_>>(),
                vec![("alias", Some(22)), ("board", Some(22))]
            );
        });
    }
}
