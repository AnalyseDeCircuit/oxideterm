use super::*;
fn container_input_uses_registry(kind: &HostContainerInputKind) -> bool {
    matches!(
        kind,
        HostContainerInputKind::Pull | HostContainerInputKind::Push
    )
}
fn container_input_title_key(kind: &HostContainerInputKind) -> &'static str {
    match kind {
        HostContainerInputKind::Run => "sidebar.host_container.inputs.run_title",
        HostContainerInputKind::Pull => "sidebar.host_container.inputs.pull_title",
        HostContainerInputKind::Push => "sidebar.host_container.inputs.push_title",
        HostContainerInputKind::Build => "sidebar.host_container.inputs.build_title",
        HostContainerInputKind::Tag { .. } => "sidebar.host_container.inputs.tag_title",
        HostContainerInputKind::Save { .. } => "sidebar.host_container.inputs.save_title",
        HostContainerInputKind::Load => "sidebar.host_container.inputs.load_title",
        HostContainerInputKind::Import => "sidebar.host_container.inputs.import_title",
        HostContainerInputKind::Export { .. } => "sidebar.host_container.inputs.export_title",
        HostContainerInputKind::Copy { .. } => "sidebar.host_container.inputs.copy_title",
        HostContainerInputKind::CreateVolume => "sidebar.host_container.inputs.volume_title",
        HostContainerInputKind::CreateNetwork => "sidebar.host_container.inputs.network_title",
        HostContainerInputKind::CreatePod => "sidebar.host_container.inputs.pod_title",
        HostContainerInputKind::Rename { .. } => "sidebar.host_container.inputs.rename_title",
        HostContainerInputKind::Login => "sidebar.host_container.inputs.login_title",
    }
}

fn container_input_placeholder_key(kind: &HostContainerInputKind) -> &'static str {
    match kind {
        HostContainerInputKind::Run => "sidebar.host_container.inputs.run_placeholder",
        HostContainerInputKind::Pull => "sidebar.host_container.inputs.pull_placeholder",
        HostContainerInputKind::Push => "sidebar.host_container.inputs.push_placeholder",
        HostContainerInputKind::Build => "sidebar.host_container.inputs.build_placeholder",
        HostContainerInputKind::Tag { .. } => "sidebar.host_container.inputs.tag_placeholder",
        HostContainerInputKind::Save { .. } => "sidebar.host_container.inputs.save_placeholder",
        HostContainerInputKind::Load => "sidebar.host_container.inputs.load_placeholder",
        HostContainerInputKind::Import => "sidebar.host_container.inputs.import_placeholder",
        HostContainerInputKind::Export { .. } => "sidebar.host_container.inputs.export_placeholder",
        HostContainerInputKind::Copy { .. } => "sidebar.host_container.inputs.copy_placeholder",
        HostContainerInputKind::CreateVolume => "sidebar.host_container.inputs.volume_placeholder",
        HostContainerInputKind::CreateNetwork => {
            "sidebar.host_container.inputs.network_placeholder"
        }
        HostContainerInputKind::CreatePod => "sidebar.host_container.inputs.pod_placeholder",
        HostContainerInputKind::Rename { .. } => "sidebar.host_container.inputs.rename_placeholder",
        HostContainerInputKind::Login => "sidebar.host_container.inputs.login_placeholder",
    }
}

fn container_input_submit_key(kind: &HostContainerInputKind) -> &'static str {
    match kind {
        HostContainerInputKind::Run => "sidebar.host_container.actions.run",
        HostContainerInputKind::Pull => "sidebar.host_container.actions.pull",
        HostContainerInputKind::Push => "sidebar.host_container.actions.push",
        HostContainerInputKind::Build => "sidebar.host_container.actions.build",
        HostContainerInputKind::Tag { .. } => "sidebar.host_container.actions.tag",
        HostContainerInputKind::Save { .. } => "sidebar.host_container.actions.save",
        HostContainerInputKind::Load => "sidebar.host_container.actions.load",
        HostContainerInputKind::Import => "sidebar.host_container.actions.import",
        HostContainerInputKind::Export { .. } => "sidebar.host_container.actions.export",
        HostContainerInputKind::Copy { .. } => "sidebar.host_container.actions.copy",
        HostContainerInputKind::CreateVolume | HostContainerInputKind::CreateNetwork => {
            "sidebar.host_container.actions.create"
        }
        HostContainerInputKind::CreatePod => "sidebar.host_container.actions.create",
        HostContainerInputKind::Rename { .. } => "sidebar.host_container.actions.rename",
        HostContainerInputKind::Login => "sidebar.host_container.actions.login",
    }
}
impl HostToolsDialogs {
    pub(super) fn render_host_container_text_dialog(
        &self,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let dialog = self.host_tools.read(cx).container_text_dialog()?;
        let (dialog_width, dialog_height) = self.host_tools.read(cx).container_text_dialog_size();
        let theme = self.tokens.ui;
        // The capture body is read-only text, but users must be able to select
        // and copy an inspect line without reopening the shell.
        let text_group_id = selection_group("host-container-capture", dialog.title.as_str());
        let mono_font_family = self.mono_font.clone();
        let content = if dialog.loading {
            div()
                .p_4()
                .text_color(rgb(theme.text_muted))
                .child(self.i18n.t("sidebar.host_container.captures.loading"))
                .into_any_element()
        } else if let Some(error) = dialog.error.as_ref() {
            div()
                .p_4()
                .text_color(rgb(MONITOR_RED))
                .child(error.clone())
                .into_any_element()
        } else {
            let mut lines = div()
                .p_3()
                .flex()
                .flex_col()
                .gap(px(1.0))
                .font_family(mono_font_family.clone())
                .text_size(px(11.0))
                .text_color(rgb(theme.text));
            let mut rendered_lines = 0;
            let mut omitted_lines = 0;
            if let Some(output) = dialog.output.as_deref() {
                // One element per line: a newline-heavy capture would otherwise
                // build millions of nodes inside the output cap.
                const MAX_RENDERED_LINES: usize = 4000;
                for (index, line) in output.lines().enumerate() {
                    if rendered_lines >= MAX_RENDERED_LINES {
                        omitted_lines = output.lines().count().saturating_sub(MAX_RENDERED_LINES);
                        break;
                    }
                    // Inspect/log output can embed secrets; lines stay bounded,
                    // non-wrapping rows and never reach notices.
                    let display_line = if line.is_empty() {
                        " ".to_string()
                    } else {
                        line.chars().take(512).collect::<String>()
                    };
                    lines = lines.child(
                        div()
                            .id(("host-container-capture-line", index))
                            .flex_none()
                            .child(self.window.selection(
                                text_group_id,
                                "host-container-capture-line",
                                index,
                                display_line,
                                theme.text,
                                Some(mono_font_family.clone()),
                                cx,
                            )),
                    );
                    rendered_lines += 1;
                }
            }
            if rendered_lines == 0 {
                lines =
                    lines.child(div().child(self.i18n.t("sidebar.host_container.captures.empty")));
            } else if omitted_lines > 0 {
                lines = lines.child(
                    div().flex_none().text_color(rgb(theme.text_muted)).child(
                        self.i18n
                            .t("sidebar.host_container.captures.truncated")
                            .replace("{{count}}", &omitted_lines.to_string()),
                    ),
                );
            }
            lines.into_any_element()
        };

        Some(
            oxideterm_gpui_ui::modal::dismissible_dialog_backdrop()
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|this, _event, _window, cx| {
                        this.host_tools.update(cx, |host_tools, cx| {
                            host_tools.dismiss_container_text_dialog(cx);
                        });
                        cx.stop_propagation();
                    }),
                )
                // Resize continuation lives on the backdrop so the corner drag
                // never depends on the pointer staying inside the handle.
                .on_mouse_move(cx.listener(|this, event: &MouseMoveEvent, _window, cx| {
                    this.host_tools.update(cx, |host_tools, cx| {
                        host_tools.update_container_text_dialog_resize(event, cx);
                    });
                }))
                .on_mouse_up(
                    MouseButton::Left,
                    cx.listener(|this, _event, _window, cx| {
                        this.host_tools.update(cx, |host_tools, cx| {
                            host_tools.finish_container_text_dialog_resize(cx);
                        });
                    }),
                )
                .child(oxideterm_gpui_ui::modal::overlay_content_boundary(
                    oxideterm_gpui_ui::modal::dialog_content(&self.tokens)
                        .relative()
                        .w(px(dialog_width))
                        .max_h(px(dialog_height + 96.0))
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
                                        .truncate()
                                        .text_size(px(14.0))
                                        .font_weight(gpui::FontWeight::MEDIUM)
                                        .text_color(rgb(theme.text))
                                        .child(dialog.title.clone()),
                                )
                                .child(host_tools_tooltip_icon_button(
                                    &self.tokens,
                                    "lucide/x.svg",
                                    14.0,
                                    rgb(theme.text_muted),
                                    IconButtonOptions {
                                        size: 24.0,
                                        has_background: true,
                                        background: Some(rgb(theme.bg_hover)),
                                        hover_background: Some(rgb(theme.bg_panel)),
                                        idle_opacity: 1.0,
                                        ..IconButtonOptions::compact(24.0)
                                    },
                                    self.i18n.t("sidebar.host_container.captures.close"),
                                    "host-container-capture-close",
                                    true,
                                    cx.listener(|this, _event, _window, cx| {
                                        this.host_tools.update(cx, |host_tools, cx| {
                                            host_tools.dismiss_container_text_dialog(cx);
                                        });
                                        cx.stop_propagation();
                                    }),
                                )),
                        )
                        .child(
                            div()
                                .id("host-container-capture-scroll")
                                .h(px(dialog_height))
                                .overflow_y_scroll()
                                .track_scroll(&dialog.scroll_handle)
                                .vertical_scrollbar(&dialog.scroll_handle)
                                .overflow_x_scroll()
                                .child(content),
                        )
                        .child(
                            // Invisible corner hotzone: drag resizes, double
                            // click restores the default size.
                            div()
                                .id("host-container-capture-resize")
                                .absolute()
                                .right(px(2.0))
                                .bottom(px(2.0))
                                .w(px(14.0))
                                .h(px(14.0))
                                .border_r_2()
                                .border_b_2()
                                .border_color(rgba((theme.text_muted << 8) | 0x66))
                                .cursor(CursorStyle::ResizeUpLeftDownRight)
                                .occlude()
                                .on_mouse_down(
                                    MouseButton::Left,
                                    cx.listener(|this, event: &MouseDownEvent, _window, cx| {
                                        this.host_tools.update(cx, |host_tools, cx| {
                                            host_tools
                                                .start_container_text_dialog_resize(event, cx);
                                        });
                                        cx.stop_propagation();
                                    }),
                                ),
                        ),
                ))
                .into_any_element(),
        )
    }
    /// Public and signed-in registries offered under the registry field.
    fn container_registry_picks(&self, cx: &App) -> Vec<String> {
        let mut picks = vec![
            oxideterm_connection_monitor::container::DEFAULT_REGISTRY.to_string(),
            "quay.io".to_string(),
        ];
        if let Some(registries) = &self.host_tools.read(cx).host_container.registries {
            for registry in registries {
                let host = registry_host(&registry.registry);
                if !host.is_empty() && !picks.iter().any(|pick| pick.eq_ignore_ascii_case(&host)) {
                    picks.push(host);
                }
            }
        }
        picks
    }

    pub(super) fn render_host_container_input_dialog(
        &self,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        // Copy the dialog's fields out of the entity first: the text-field
        // helpers need `&mut Context`, which cannot overlap the read borrow.
        let (kind, uses_registry, registry_value, value, run) = {
            let host_tools = self.host_tools.read(cx);
            let ui = &host_tools.ui;
            let dialog = ui.host_container_input_dialog.as_ref()?;
            (
                dialog.kind.clone(),
                container_input_uses_registry(&dialog.kind),
                dialog.registry.clone(),
                dialog.value.clone(),
                dialog.run.clone(),
            )
        };
        let registry_control = uses_registry.then(|| {
            self.input(
                HostToolsTextInput::ContainerDialogRegistry,
                registry_value.as_str(),
                self.i18n
                    .t("sidebar.host_container.inputs.registry_placeholder"),
                34.0,
                cx,
            )
        });
        let input_control = self.input(
            HostToolsTextInput::ContainerDialog,
            value.as_str(),
            self.i18n.t(container_input_placeholder_key(&kind)),
            34.0,
            cx,
        );
        let theme = self.tokens.ui;
        let run_controls = (kind == HostContainerInputKind::Run).then(|| {
            let mut run_fields = div().w_full().flex().flex_col().gap_2();
            run_fields = run_fields.child(
                self.input(
                    HostToolsTextInput::ContainerRunName,
                    run.name.as_str(),
                    self.i18n
                        .t("sidebar.host_container.inputs.run_name_placeholder"),
                    34.0,
                    cx,
                ),
            );
            let flag_chip = |id: &'static str,
                             label: String,
                             active: bool,
                             flag: HostContainerRunFlag,
                             cx: &mut Context<Self>| {
                div()
                    .id(id)
                    .h(px(24.0))
                    .px_2()
                    .flex()
                    .items_center()
                    .rounded(px(self.tokens.radii.md))
                    .cursor_pointer()
                    .text_size(px(10.0))
                    .when(active, |chip| {
                        chip.bg(rgb(theme.bg_panel))
                            .border_1()
                            .border_color(rgba((theme.accent << 8) | MONITOR_BORDER_ALPHA))
                            .text_color(rgb(theme.text))
                    })
                    .when(!active, |chip| {
                        chip.text_color(rgb(theme.text_muted))
                            .hover(|chip| chip.bg(rgb(theme.bg_hover)))
                    })
                    .child(label)
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |this, _event, _window, cx| {
                            this.host_tools.update(cx, |host_tools, cx| {
                                host_tools.set_container_run_flag(flag, !active, cx);
                            });
                            cx.stop_propagation();
                        }),
                    )
            };
            let mut run_flags = div().flex().items_center().gap_1();
            run_flags = run_flags.child(flag_chip(
                "host-container-run-detached",
                self.i18n.t("sidebar.host_container.inputs.run_detached"),
                run.detached,
                HostContainerRunFlag::Detached,
                cx,
            ));
            run_flags = run_flags.child(flag_chip(
                "host-container-run-interactive",
                self.i18n.t("sidebar.host_container.inputs.run_interactive"),
                run.interactive,
                HostContainerRunFlag::Interactive,
                cx,
            ));
            run_flags = run_flags.child(flag_chip(
                "host-container-run-remove",
                self.i18n.t("sidebar.host_container.inputs.run_auto_remove"),
                run.auto_remove,
                HostContainerRunFlag::AutoRemove,
                cx,
            ));
            run_fields = run_fields.child(run_flags);
            run_fields = run_fields.child(
                self.input(
                    HostToolsTextInput::ContainerRunPorts,
                    run.ports.as_str(),
                    self.i18n
                        .t("sidebar.host_container.inputs.run_ports_placeholder"),
                    64.0,
                    cx,
                ),
            );
            run_fields = run_fields.child(
                self.input(
                    HostToolsTextInput::ContainerRunVolumes,
                    run.volumes.as_str(),
                    self.i18n
                        .t("sidebar.host_container.inputs.run_volumes_placeholder"),
                    64.0,
                    cx,
                ),
            );
            run_fields = run_fields.child(
                self.input(
                    HostToolsTextInput::ContainerRunCommand,
                    run.command.as_str(),
                    self.i18n
                        .t("sidebar.host_container.inputs.run_command_placeholder"),
                    34.0,
                    cx,
                ),
            );
            run_fields
        });
        let submit_disabled = value.trim().is_empty();
        let registry_picks = if uses_registry {
            self.container_registry_picks(cx)
        } else {
            Vec::new()
        };
        let cancel_action = self.footer_button(
            self.i18n.t("sidebar.host_container.confirm.cancel"),
            ButtonVariant::Secondary,
            ConfirmDialogAction::Cancel,
            false,
            None,
            |this, _event, _window, cx| {
                this.host_tools.update(cx, |host_tools, cx| {
                    host_tools.dismiss_container_input_dialog(cx);
                });
                this.window.clear_composition(cx);
                cx.notify();
            },
            cx,
        );
        let submit_action = self.footer_button(
            self.i18n.t(container_input_submit_key(&kind)),
            ButtonVariant::Default,
            ConfirmDialogAction::Confirm,
            submit_disabled,
            None,
            |this, _event, window, cx| {
                this.window.submit_container(window, cx);
            },
            cx,
        );
        Some(
            oxideterm_gpui_ui::modal::dismissible_dialog_backdrop()
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|this, _event, _window, cx| {
                        this.host_tools.update(cx, |host_tools, cx| {
                            host_tools.dismiss_container_input_dialog(cx);
                        });
                        cx.stop_propagation();
                    }),
                )
                .child(oxideterm_gpui_ui::modal::overlay_content_boundary(
                    oxideterm_gpui_ui::modal::dialog_content(&self.tokens)
                        .w(px(560.0))
                        .child(
                            div()
                                .px_4()
                                .py_3()
                                .border_b_1()
                                .border_color(rgb(theme.border))
                                .child(
                                    div()
                                        .text_size(px(14.0))
                                        .font_weight(gpui::FontWeight::MEDIUM)
                                        .text_color(rgb(theme.text))
                                        .child(self.i18n.t(container_input_title_key(&kind))),
                                ),
                        )
                        .child(
                            div()
                                .p_4()
                                .flex()
                                .flex_col()
                                .gap_2()
                                .when_some(registry_control, |body, registry_control| {
                                    body.child(registry_control).child(
                                        div()
                                            .flex()
                                            .flex_wrap()
                                            .items_center()
                                            .gap_1()
                                            .children(registry_picks.iter().enumerate().map(
                                                |(index, pick)| {
                                                    let pick_value = pick.clone();
                                                    div()
                                                        .id(("host-container-registry-pick", index))
                                                        .px_2()
                                                        .py(px(2.0))
                                                        .rounded(px(self.tokens.radii.sm))
                                                        .cursor_pointer()
                                                        .text_size(px(10.0))
                                                        .text_color(rgb(theme.text_muted))
                                                        .bg(rgb(theme.bg_hover))
                                                        .hover(move |chip| {
                                                            chip.bg(rgb(theme.bg_panel))
                                                        })
                                                        .child(pick.clone())
                                                        .on_mouse_down(
                                                            MouseButton::Left,
                                                            cx.listener(
                                                                move |this,
                                                                      _event,
                                                                      _window,
                                                                      cx| {
                                                                    this.host_tools.update(
                                                                        cx,
                                                                        |host_tools, cx| {
                                                                            if let Some(dialog) = host_tools
                                                                                .ui
                                                                                .host_container_input_dialog
                                                                                .as_mut()
                                                                            {
                                                                                dialog.registry =
                                                                                    pick_value
                                                                                        .clone();
                                                                            }
                                                                            cx.notify();
                                                                        },
                                                                    );
                                                                    cx.stop_propagation();
                                                                },
                                                            ),
                                                        )
                                                        .into_any_element()
                                                },
                                            )),
                                    )
                                })
                                .child(input_control)
                                .when_some(run_controls, |body, controls| {
                                    body.child(controls)
                                })
                        )
                        .child(
                            div()
                                .px_4()
                                .py_3()
                                .border_t_1()
                                .border_color(rgb(theme.border))
                                .flex()
                                .justify_end()
                                .gap_2()
                                .child(cancel_action)
                                .child(submit_action),
                        ),
                ))
                .into_any_element(),
        )
    }
}
