use super::*;

impl HostToolsEntity {
    // ---- verbs ----

    /// One button per verb the provider implements for this row, and only when
    /// the model can build a command for it.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn mux_verb_buttons(
        &self,
        host: &str,
        provider: Multiplexer,
        session: &str,
        node: Option<&MuxNode>,
        session_running: bool,
        windows: bool,
        icon_size: f32,
        tokens: &ThemeTokens,
        i18n: &I18n,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        let theme = tokens.ui;
        provider
            .verbs_for(node.map(|node| node.kind))
            .iter()
            .filter_map(|verb_kind| {
                let kind = *verb_kind;
                if !self.mux_verb_supported(provider, windows, session, node, kind) {
                    return None;
                }
                let disabled = self.mux_action_running()
                    || match kind {
                        // Stop needs a live session, delete needs a stopped one.
                        MuxVerbKind::Stop => !session_running,
                        MuxVerbKind::Delete => session_running,
                        _ => false,
                    };
                let (icon, label_key, danger) = mux_verb_view(kind, node.map(|node| node.kind));
                let node = node.cloned();
                let dialog_kind = mux_dialog_kind(kind, node.as_ref());
                let prefill = mux_verb_prefill(kind, node.as_ref());
                let host = host.to_owned();
                let session = session.to_owned();
                let target_label = node
                    .as_ref()
                    .map(|node| node.label.clone())
                    .unwrap_or_else(|| session.clone());
                Some(host_tools_tooltip_icon_button(
                    tokens,
                    icon,
                    icon_size,
                    rgb(if danger { MONITOR_RED } else { theme.text }),
                    oxideterm_gpui_ui::button::IconButtonOptions {
                        size: 22.0,
                        disabled,
                        has_background: true,
                        background: Some(if danger {
                            rgba((MONITOR_RED << 8) | MONITOR_TINT_ALPHA)
                        } else {
                            rgb(theme.bg_hover)
                        }),
                        hover_background: Some(if danger {
                            rgba((MONITOR_RED << 8) | 0x30)
                        } else {
                            rgb(theme.bg_panel)
                        }),
                        idle_opacity: 1.0,
                        ..oxideterm_gpui_ui::button::IconButtonOptions::compact(22.0)
                    },
                    i18n.t(label_key),
                    mux_verb_element_id(kind),
                    true,
                    cx.listener(move |tools, _event, window, cx| {
                        cx.stop_propagation();
                        match kind {
                            MuxVerbKind::Read => {
                                if let Some(node) = node.as_ref() {
                                    tools.request_mux_read(&session, node, cx);
                                }
                            }
                            MuxVerbKind::Rename
                            | MuxVerbKind::Run
                            | MuxVerbKind::CreateWorkspace => {
                                // The payload verbs collect their value first;
                                // a dialog without a target can send nothing.
                                let Some(dialog) = dialog_kind.clone() else {
                                    cx.emit(HostToolsEvent::ShowNotice(
                                        HostToolsNotice::MuxActionFailed,
                                    ));
                                    return;
                                };
                                tools.open_mux_input_dialog(dialog, &session, &prefill, window, cx);
                            }
                            _ => {
                                let request = MuxActionRequest {
                                    provider,
                                    connection_id: host.clone(),
                                    session: session.clone(),
                                    target_label: target_label.clone(),
                                    node: node.clone(),
                                    verb: mux_probe_verb(kind),
                                };
                                if matches!(
                                    kind,
                                    MuxVerbKind::Close | MuxVerbKind::Stop | MuxVerbKind::Delete
                                ) {
                                    if let Some(notice) = tools.open_mux_confirm(request, cx) {
                                        cx.emit(HostToolsEvent::ShowNotice(notice));
                                    } else {
                                        // The root owns only the shared focus
                                        // adapter, not the confirm state, so an
                                        // open confirm leaves keyboard focus on
                                        // the previous Confirm button and Enter
                                        // would run the next destructive verb
                                        // unasked.
                                        window.dispatch_action(
                                            Box::new(HostToolsWindowRequest::new(
                                                HostToolsWindowIntent::PrepareMuxConfirm,
                                            )),
                                            cx,
                                        );
                                    }
                                } else {
                                    tools.dispatch_mux_verb(request, cx);
                                }
                            }
                        }
                    }),
                ))
            })
            .collect()
    }

    fn dispatch_mux_verb(&mut self, request: MuxActionRequest, cx: &mut Context<Self>) {
        let Some(runtime) = self.lifecycle_runtime.clone() else {
            cx.emit(HostToolsEvent::ShowNotice(
                HostToolsNotice::MuxConnectionMissing,
            ));
            return;
        };
        for notice in self.run_mux_action(request, runtime, cx) {
            cx.emit(HostToolsEvent::ShowNotice(notice));
        }
    }

    // ---- input dialog ----

    pub(crate) fn open_mux_input_dialog(
        &mut self,
        kind: MuxInputDialogKind,
        session: &str,
        prefill: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(provider) = self.mux_provider() else {
            return;
        };
        self.ui.host_mux_input_dialog = Some(MuxInputDialog {
            kind,
            session: session.to_string(),
            // The dialog belongs to the host it was opened for: a lifecycle
            // fallback can move the panel to another connection while it is
            // open, and submitting then would mutate the wrong machine.
            provider,
            connection_id: self.host_tmux.mux.host.clone(),
            value: zeroize::Zeroizing::new(prefill.to_string()),
            error: None,
        });
        self.ui.focus_input(HostToolsTextInput::MuxDialog);
        // Sidebar clicks do not move keyboard focus; without this handoff the
        // typed run command would keep going to the focused terminal.
        window.dispatch_action(
            Box::new(HostToolsWindowRequest::new(
                HostToolsWindowIntent::PrepareMuxInputDialog,
            )),
            cx,
        );
        cx.notify();
    }

    pub fn dismiss_mux_input_dialog(&mut self, cx: &mut Context<Self>) {
        if self.ui.host_mux_input_dialog.take().is_some() {
            if self.ui.input_is_focused(HostToolsTextInput::MuxDialog) {
                self.ui.clear_input_focus();
            }
            cx.notify();
        }
    }

    pub fn submit_mux_input_from_view(&mut self, cx: &mut Context<Self>) {
        // A rejected payload must leave the typed text in front of the user, so
        // the dialog is only consumed once the model accepts the command. The
        // reason is answered under the input, not by a toast over the panel.
        let blank = self
            .ui
            .host_mux_input_dialog
            .as_ref()
            .is_none_or(|dialog| dialog.value.trim().is_empty());
        if blank {
            if let Some(dialog) = self.ui.host_mux_input_dialog.as_mut() {
                dialog.error = Some("sidebar.multiplexer.inputs.error_required");
                cx.notify();
            }
            return;
        }
        if self.host_tmux.mux.action_running.is_some() {
            cx.emit(HostToolsEvent::ShowNotice(
                HostToolsNotice::MuxActionAlreadyRunning,
            ));
            cx.notify();
            return;
        }
        let Some(runtime) = self.lifecycle_runtime.clone() else {
            cx.emit(HostToolsEvent::ShowNotice(
                HostToolsNotice::MuxConnectionMissing,
            ));
            return;
        };
        let Some(provider) = self.mux_provider() else {
            return;
        };
        if self.host_tmux.mux.host.is_empty() {
            cx.emit(HostToolsEvent::ShowNotice(
                HostToolsNotice::MuxConnectionMissing,
            ));
            return;
        }
        let connection_id = self.host_tmux.mux.host.clone();
        let Some(mut dialog) = self.ui.host_mux_input_dialog.take() else {
            return;
        };
        if dialog.provider != provider || dialog.connection_id != connection_id {
            // The panel moved to another host or provider while the dialog was
            // open. The typed value describes an object this host does not
            // have, so nothing runs and the dialog goes away with it.
            drop(dialog);
            self.ui.clear_input_focus();
            cx.emit(HostToolsEvent::ShowNotice(
                HostToolsNotice::MuxConnectionMissing,
            ));
            cx.notify();
            return;
        }
        dialog.error = None;
        self.ui.clear_input_focus();
        let value = dialog.value.trim();
        let windows = self.mux_windows(&connection_id);
        let (verb, target_label, node) = match &dialog.kind {
            MuxInputDialogKind::NewWorkspace => {
                // "label path" packs both fields into one input line.
                let mut parts = value.splitn(2, char::is_whitespace);
                let label = parts.next().unwrap_or_default().to_string();
                let cwd = parts.next().unwrap_or_default().trim().to_string();
                (
                    MuxVerb::CreateWorkspace {
                        label: label.clone(),
                        cwd: (!cwd.is_empty()).then_some(cwd),
                    },
                    label,
                    None,
                )
            }
            MuxInputDialogKind::RenameWorkspace { target }
            | MuxInputDialogKind::RenameTab { target }
            | MuxInputDialogKind::RenamePane { target } => (
                MuxVerb::Rename {
                    label: value.to_owned(),
                },
                value.to_owned(),
                Some(target.clone()),
            ),
            MuxInputDialogKind::RunPane { target } => (
                MuxVerb::Run {
                    command: zeroize::Zeroizing::new(value.to_owned()),
                },
                target.label.clone(),
                Some(target.clone()),
            ),
            // A new session is a terminal command, dispatched by the view.
            MuxInputDialogKind::NewSession => return,
        };
        // The model owns the payload grammar, so it decides whether this value
        // can become a command at all; a refusal reopens the dialog with the
        // text and names the reason where it was typed.
        let Some(command) = provider.verb_command(windows, &dialog.session, node.as_ref(), &verb)
        else {
            dialog.error = Some("sidebar.multiplexer.inputs.error_rejected");
            self.ui.host_mux_input_dialog = Some(dialog);
            cx.notify();
            return;
        };
        for notice in self.start_mux_action(
            MuxActionRequest {
                provider,
                connection_id,
                session: dialog.session,
                target_label,
                node,
                verb,
            },
            command,
            runtime,
            cx,
        ) {
            cx.emit(HostToolsEvent::ShowNotice(notice));
        }
        cx.notify();
    }

    // ---- confirm ----

    pub fn submit_mux_input(
        &mut self,
        i18n: &I18n,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let new_session = self
            .ui
            .host_mux_input_dialog
            .as_ref()
            .filter(|dialog| matches!(dialog.kind, MuxInputDialogKind::NewSession))
            .map(|dialog| zeroize::Zeroizing::new(dialog.value.trim().to_string()));
        let Some(name) = new_session else {
            self.submit_mux_input_from_view(cx);
            return true;
        };
        if name.is_empty() {
            return false;
        }
        let host = self.host_tmux.mux.host.clone();
        let Some(provider) = self.host_tmux.mux.provider else {
            return false;
        };
        let Some(command) = provider.new_session_command(self.mux_windows(&host), &name) else {
            self.dismiss_mux_input_dialog(cx);
            cx.emit(HostToolsEvent::ShowNotice(HostToolsNotice::MuxActionFailed));
            return false;
        };

        // The workspace still opens the terminal through its one-shot window intent.
        let title = mux_row_title(provider, &name);
        let opened = i18n.t("sidebar.multiplexer.toast.attach_opened");
        let missing = i18n.t("sidebar.multiplexer.toast.connection_missing");
        self.dismiss_mux_input_dialog(cx);
        self.dispatch_mux_terminal(&host, command, title, opened, missing, window, cx);
        true
    }

    pub fn render_mux_read_dialog(
        &self,
        tokens: &ThemeTokens,
        i18n: &I18n,
        mono_font_family: SharedString,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let dialog = self.mux_read_dialog()?;
        let theme = tokens.ui;
        let content = if dialog.loading {
            div()
                .p_4()
                .text_color(rgb(theme.text_muted))
                .child(i18n.t("sidebar.multiplexer.read.loading"))
                .into_any_element()
        } else if dialog.failed {
            // A failed capture is answered with the localized fallback only.
            div()
                .p_4()
                .text_color(rgb(MONITOR_RED))
                .child(i18n.t("sidebar.multiplexer.toast.read_failed"))
                .into_any_element()
        } else {
            let output = dialog.output.as_deref();
            let mut lines = div()
                .p_3()
                .flex()
                .flex_col()
                .gap(px(1.0))
                .font_family(mono_font_family)
                .text_size(px(11.0))
                .text_color(rgb(theme.text));
            let mut rendered = 0;
            if let Some(output) = output {
                for (index, line) in output.lines().enumerate() {
                    let display_line = if line.is_empty() {
                        " ".to_string()
                    } else {
                        line.chars().take(512).collect::<String>()
                    };
                    lines = lines.child(
                        div()
                            .id(("host-mux-read-line", index))
                            .flex_none()
                            .whitespace_nowrap()
                            .child(display_line),
                    );
                    rendered += 1;
                }
            }
            if rendered == 0 {
                lines = lines.child(div().child(i18n.t("sidebar.multiplexer.read.empty")));
            }
            lines.into_any_element()
        };

        Some(
            oxideterm_gpui_ui::modal::dismissible_dialog_backdrop()
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|this, _event, _window, cx| {
                        this.dismiss_mux_read_dialog(cx);
                        cx.stop_propagation();
                    }),
                )
                .child(oxideterm_gpui_ui::modal::overlay_content_boundary(
                    oxideterm_gpui_ui::modal::dialog_content(tokens)
                        .w(px(760.0))
                        .max_h(px(520.0))
                        .child(
                            div()
                                .flex_none()
                                .px_4()
                                .py_3()
                                .border_b_1()
                                .border_color(rgb(theme.border))
                                .flex()
                                .items_center()
                                .justify_between()
                                .gap_3()
                                .child(
                                    div()
                                        .min_w_0()
                                        .flex()
                                        .flex_col()
                                        .gap_1()
                                        .child(
                                            div()
                                                .truncate()
                                                .text_size(px(14.0))
                                                .font_weight(gpui::FontWeight::MEDIUM)
                                                .text_color(rgb(theme.text))
                                                .child(
                                                    i18n.t("sidebar.multiplexer.read.title")
                                                        .replace(
                                                            "{{pane}}",
                                                            &dialog.request.target_label,
                                                        ),
                                                ),
                                        )
                                        .child(
                                            div()
                                                .truncate()
                                                .text_size(px(11.0))
                                                .text_color(rgb(theme.text_muted))
                                                .child(dialog.request.target_id.clone()),
                                        ),
                                )
                                .child(host_tools_tooltip_icon_button(
                                    tokens,
                                    "lucide/x.svg",
                                    14.0,
                                    rgb(theme.text_muted),
                                    oxideterm_gpui_ui::button::IconButtonOptions {
                                        size: 24.0,
                                        has_background: true,
                                        background: Some(rgb(theme.bg_hover)),
                                        hover_background: Some(rgb(theme.bg_panel)),
                                        idle_opacity: 1.0,
                                        ..oxideterm_gpui_ui::button::IconButtonOptions::compact(
                                            24.0,
                                        )
                                    },
                                    i18n.t("sidebar.multiplexer.read.close"),
                                    "host-mux-read-close",
                                    true,
                                    cx.listener(|this, _event, _window, cx| {
                                        this.dismiss_mux_read_dialog(cx);
                                        cx.stop_propagation();
                                    }),
                                )),
                        )
                        .child(
                            div()
                                .id("host-mux-read-scroll")
                                .min_h(px(120.0))
                                .max_h(px(436.0))
                                .overflow_y_scroll()
                                .overflow_x_scroll()
                                .child(content),
                        ),
                ))
                .into_any_element(),
        )
    }

    pub fn render_mux_confirm_view(
        &self,
        tokens: &ThemeTokens,
        i18n: &I18n,
        focused_action: Option<ConfirmDialogAction>,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let (request, phase) = self.mux_confirm_view()?;
        let description = i18n
            .t(mux_confirm_description_key(&request))
            .replace("{{name}}", &request.target_label)
            .replace("{{session}}", &request.session)
            .replace(
                "{{target}}",
                request
                    .node
                    .as_ref()
                    .map(|node| node.id.as_str())
                    .unwrap_or_default(),
            );
        let exit_delay = oxideterm_gpui_ui::motion::duration(
            tokens,
            oxideterm_gpui_ui::motion::MotionDuration::Control,
        );
        let confirm_delay = exit_delay;
        Some(
            oxideterm_gpui_ui::confirm::confirm_dialog_with_focus_motion(
                tokens,
                "host-mux-confirm-motion",
                phase,
                ConfirmDialogView {
                    variant: ConfirmDialogVariant::Danger,
                    title: div()
                        .child(i18n.t("sidebar.multiplexer.confirm.title"))
                        .into_any_element(),
                    description: Some(div().child(description).into_any_element()),
                    cancel_label: div()
                        .child(i18n.t("sidebar.multiplexer.confirm.cancel"))
                        .into_any_element(),
                    confirm_label: div()
                        .child(i18n.t("sidebar.multiplexer.confirm.run"))
                        .into_any_element(),
                },
                focused_action,
                cx.listener(move |tools, _event, _window, cx| {
                    // The global keyboard-focus marker is transient render
                    // input; the confirmation lifecycle belongs to Host Tools.
                    tools.begin_mux_confirm_exit(exit_delay, cx);
                }),
                cx.listener(move |tools, _event, _window, cx| {
                    tools.confirm_mux_action_from_view(confirm_delay, cx);
                }),
            )
            .into_any_element(),
        )
    }
}
