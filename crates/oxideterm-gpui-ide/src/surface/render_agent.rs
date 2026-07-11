use oxideterm_gpui_ui::modal::rounded_shell_child_radius;

impl IdeSurface {
    fn render_status_bar(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let dirty_count = self
            .workspace
            .tabs()
            .iter()
            .filter(|tab| self.is_tab_dirty(tab.id, cx))
            .count();
        let active_path = self
            .workspace
            .active_tab()
            .and_then(|tab_id| self.workspace.buffer(tab_id))
            .map(|buffer| match &buffer.location {
                IdeLocation::Remote { path, .. } => path.clone(),
                IdeLocation::Local { path } => path.display().to_string(),
            })
            .unwrap_or_default();

        div()
            .h(px(IDE_STATUS_BAR_HEIGHT))
            .px_2()
            .flex()
            .items_center()
            .justify_between()
            .border_t_1()
            .border_color(rgb(self.tokens.ui.border))
            .bg(if self.runtime_settings.background_active {
                rgba((self.tokens.ui.bg_panel << 8) | IDE_BG_ACTIVE_THEME_ALPHA)
            } else {
                rgb(self.tokens.ui.bg_panel)
            })
            .text_size(px(self.tokens.metrics.ui_text_xs))
            .text_color(rgb(self.tokens.ui.text_muted))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(self.render_agent_status_trigger(cx))
                    .when_some(self.git_branch.clone(), |this, branch| {
                        this.child(format!("git: {branch}"))
                    })
                    .when(dirty_count > 0, |this| {
                        this.child(format!("{dirty_count} unsaved"))
                    }),
            )
            .child(div().truncate().child(active_path))
            .into_any_element()
    }

    fn render_agent_status_trigger(&self, cx: &mut Context<Self>) -> AnyElement {
        let status = self.fs.status_for_node(self.node_id.as_deref());
        let (icon, label, color, opacity) = self.agent_status_trigger_parts(&status);
        let entity = cx.entity();
        let trigger = div()
            .flex()
            .items_center()
            .gap_1()
            .mr_4()
            .px(px(6.0))
            .py(px(2.0))
            .rounded(px(self.tokens.radii.xs))
            .text_color(rgb(color))
            .opacity(opacity)
            .cursor_pointer()
            .hover(|style| style.bg(rgb(self.tokens.ui.bg_hover)))
            .child(self.icon(icon, 12.0, color))
            .child(label)
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _event: &MouseDownEvent, _window, cx| {
                    let Some(trigger_bounds) = this.agent_status_trigger_bounds else {
                        return;
                    };
                    this.agent_status_menu = Some(AgentStatusMenu { trigger_bounds });
                    this.tab_context_menu = None;
                    this.tree_context_menu = None;
                    cx.stop_propagation();
                    cx.notify();
                }),
            );

        select_anchor_probe(
            SelectAnchorId::IdeAgentStatus,
            trigger,
            move |anchor, _window, cx| {
                let _ = entity.update(cx, |this, cx| {
                    if this.agent_status_trigger_bounds != Some(anchor.bounds) {
                        this.agent_status_trigger_bounds = Some(anchor.bounds);
                        if this.agent_status_menu.is_some() {
                            this.agent_status_menu = Some(AgentStatusMenu {
                                trigger_bounds: anchor.bounds,
                            });
                            cx.notify();
                        }
                    }
                });
            },
        )
        .into_any_element()
    }

    fn agent_status_trigger_parts(&self, status: &AgentStatus) -> (&'static str, String, u32, f32) {
        if self.agent_action == Some(AgentActionKind::Refresh) {
            return (
                "lucide/hard-drive.svg",
                "...".to_string(),
                self.tokens.ui.text_muted,
                0.5,
            );
        }
        match status {
            AgentStatus::Ready { .. } => (
                "lucide/cpu.svg",
                "Agent".to_string(),
                TAILWIND_EMERALD_400,
                1.0,
            ),
            AgentStatus::Deploying => (
                "lucide/hard-drive.svg",
                "Agent...".to_string(),
                TAILWIND_AMBER_400,
                1.0,
            ),
            AgentStatus::ManualUploadRequired { .. } | AgentStatus::ManualUpdateRequired { .. } => {
                (
                    "lucide/hard-drive.svg",
                    "SFTP".to_string(),
                    TAILWIND_AMBER_400,
                    1.0,
                )
            }
            AgentStatus::Failed { .. }
            | AgentStatus::UnsupportedArch { .. }
            | AgentStatus::NotDeployed
            | AgentStatus::SftpFallback => (
                "lucide/hard-drive.svg",
                "SFTP".to_string(),
                self.tokens.ui.text_muted,
                1.0,
            ),
        }
    }

    fn render_agent_status_menu(
        &self,
        menu: AgentStatusMenu,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let status = self.fs.status_for_node(self.node_id.as_deref());
        let manual = matches!(
            status,
            AgentStatus::ManualUploadRequired { .. } | AgentStatus::ManualUpdateRequired { .. }
        );
        let width = if manual {
            IDE_AGENT_MENU_MANUAL_WIDTH
        } else {
            IDE_AGENT_MENU_WIDTH
        };
        let height = self.agent_status_menu_height(&status);
        let x = f32::from(menu.trigger_bounds.left())
            .min(f32::from(_window.viewport_size().width) - width - 8.0)
            .max(8.0);
        let y = (f32::from(menu.trigger_bounds.top()) - height - 6.0).max(8.0);
        let popup = div()
            .w(px(width))
            .py(px(IDE_AGENT_MENU_PADDING_Y))
            .rounded(px(self.tokens.radii.md))
            .border_1()
            .border_color(rgb(self.tokens.ui.border))
            .bg(rgb(self.tokens.ui.bg_panel))
            .shadow_lg()
            .text_size(px(self.tokens.metrics.ui_text_xs))
            .text_color(rgb(self.tokens.ui.text))
            .occlude()
            .child(
                div()
                    .px(px(IDE_AGENT_MENU_DESCRIPTION_PADDING_X))
                    .py(px(IDE_AGENT_MENU_DESCRIPTION_PADDING_Y))
                    .text_color(rgb(self.tokens.ui.text_muted))
                    .child(self.agent_status_description(&status)),
            )
            .when(manual, |this| {
                this.child(self.render_agent_manual_body(&status))
            })
            .child(self.render_agent_status_menu_divider())
            .when(self.agent_can_deploy(&status), |this| {
                this.child(self.render_agent_status_menu_item(
                    if self.agent_action == Some(AgentActionKind::Deploy) {
                        self.spinner_icon(
                            "ide-agent-deploy-loading",
                            12.0,
                            self.tokens.ui.text,
                        )
                    } else {
                        self.icon("lucide/rocket.svg", 12.0, self.tokens.ui.text)
                    },
                    if matches!(
                        status,
                        AgentStatus::ManualUploadRequired { .. }
                            | AgentStatus::ManualUpdateRequired { .. }
                    ) {
                        self.labels.agent_retry_btn.clone()
                    } else {
                        self.labels.agent_deploy_btn.clone()
                    },
                    false,
                    cx.listener(|this, _event, _window, cx| {
                        this.agent_status_menu = None;
                        this.start_deploy_agent(cx);
                        cx.stop_propagation();
                    }),
                ))
            })
            .when(matches!(status, AgentStatus::Ready { .. }), |this| {
                this.child(self.render_agent_status_menu_item(
                    if self.agent_action == Some(AgentActionKind::Remove) {
                        self.spinner_icon(
                            "ide-agent-remove-loading",
                            12.0,
                            TAILWIND_RED_400,
                        )
                    } else {
                        self.icon("lucide/trash-2.svg", 12.0, TAILWIND_RED_400)
                    },
                    self.labels.agent_remove_btn.clone(),
                    true,
                    cx.listener(|this, _event, _window, cx| {
                        this.agent_status_menu = None;
                        this.agent_remove_confirm_open = true;
                        cx.stop_propagation();
                        cx.notify();
                    }),
                ))
            })
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .into_any_element();

        popover_backdrop()
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _event, _window, cx| {
                    this.agent_status_menu = None;
                    cx.stop_propagation();
                    cx.notify();
                }),
            )
            .on_mouse_down(
                MouseButton::Right,
                cx.listener(|this, _event, _window, cx| {
                    this.agent_status_menu = None;
                    cx.stop_propagation();
                    cx.notify();
                }),
            )
            .child(
                deferred(
                    anchored()
                        .anchor(Corner::TopLeft)
                        .position(gpui::point(px(x), px(y)))
                        .position_mode(AnchoredPositionMode::Window)
                        .child(popup),
                )
                .with_priority(IDE_AGENT_MENU_Z),
            )
            .into_any_element()
    }

    fn agent_status_menu_height(&self, status: &AgentStatus) -> f32 {
        let action_rows =
            if matches!(status, AgentStatus::Ready { .. }) || self.agent_can_deploy(status) {
                1.0
            } else {
                0.0
            };
        let manual_body = if matches!(
            status,
            AgentStatus::ManualUploadRequired { .. } | AgentStatus::ManualUpdateRequired { .. }
        ) {
            116.0
        } else {
            0.0
        };
        8.0 + 28.0 + manual_body + 1.0 + action_rows * IDE_AGENT_MENU_ITEM_HEIGHT
    }

    fn agent_can_deploy(&self, status: &AgentStatus) -> bool {
        !matches!(status, AgentStatus::Ready { .. } | AgentStatus::Deploying)
    }

    fn agent_status_description(&self, status: &AgentStatus) -> String {
        if self.agent_action == Some(AgentActionKind::Refresh) {
            return self.labels.agent_checking.clone();
        }
        match status {
            AgentStatus::Ready { version, .. } => {
                format!("{} (v{version})", self.labels.agent_ready)
            }
            AgentStatus::Deploying => self.labels.agent_deploying.clone(),
            AgentStatus::ManualUploadRequired { .. } => self.labels.agent_manual_upload.clone(),
            AgentStatus::ManualUpdateRequired { .. } => self.labels.agent_manual_update.clone(),
            AgentStatus::Failed { reason } => format!("{}: {reason}", self.labels.sftp_mode),
            AgentStatus::UnsupportedArch { arch } => format!("{} ({arch})", self.labels.sftp_mode),
            AgentStatus::NotDeployed | AgentStatus::SftpFallback => self.labels.sftp_mode.clone(),
        }
    }

    fn render_agent_manual_body(&self, status: &AgentStatus) -> AnyElement {
        let mut body = div()
            .max_w(px(IDE_AGENT_MENU_MANUAL_WIDTH))
            .px(px(IDE_AGENT_MENU_DESCRIPTION_PADDING_X))
            .pb(px(IDE_AGENT_MENU_DESCRIPTION_PADDING_Y))
            .flex()
            .flex_col()
            .gap_2()
            .text_size(px(self.tokens.metrics.ui_text_xs))
            .text_color(rgb(self.tokens.ui.text_muted));

        match status {
            AgentStatus::ManualUploadRequired { arch, remote_path } => {
                body = body
                    .child(
                        self.render_agent_manual_hint(self.labels.agent_manual_upload_hint.clone()),
                    )
                    .child(
                        self.render_agent_manual_code(
                            self.labels.agent_upload_to.clone(),
                            remote_path,
                        ),
                    )
                    .child(
                        self.labels
                            .agent_manual_upload_arch
                            .replace("{{arch}}", arch),
                    );
            }
            AgentStatus::ManualUpdateRequired {
                arch: _,
                remote_path,
                current_agent_version,
                current_compatibility_version,
                expected_compatibility_version,
            } => {
                body = body
                    .child(
                        self.render_agent_manual_hint(self.labels.agent_manual_update_hint.clone()),
                    )
                    .child(
                        self.render_agent_manual_code(
                            self.labels.agent_upload_to.clone(),
                            remote_path,
                        ),
                    )
                    .child(
                        self.labels
                            .agent_manual_update_current_agent_version
                            .replace("{{version}}", current_agent_version),
                    )
                    .child(
                        self.labels
                            .agent_manual_update_current_compatibility_version
                            .replace("{{version}}", &current_compatibility_version.to_string()),
                    )
                    .child(
                        self.labels
                            .agent_manual_update_expected_compatibility_version
                            .replace("{{version}}", &expected_compatibility_version.to_string()),
                    );
            }
            _ => {}
        }
        body.into_any_element()
    }

    fn render_agent_manual_hint(&self, text: String) -> AnyElement {
        div()
            .flex()
            .items_start()
            .gap_2()
            .child(self.icon("lucide/info.svg", 14.0, TAILWIND_AMBER_400))
            .child(div().flex_1().child(text))
            .into_any_element()
    }

    fn render_agent_manual_code(&self, label: String, path: &str) -> AnyElement {
        div()
            .flex()
            .flex_col()
            .gap_1()
            .child(label)
            .child(
                div()
                    .px_2()
                    .py_1()
                    .rounded(px(self.tokens.radii.xs))
                    .font_family(SharedString::from(
                        self.tokens.metrics.markdown_code_font_family,
                    ))
                    .bg(rgb(self.tokens.ui.bg_sunken))
                    .text_color(rgb(self.tokens.ui.text))
                    .child(path.to_string()),
            )
            .into_any_element()
    }

    fn render_agent_status_menu_item(
        &self,
        icon: AnyElement,
        label: String,
        danger: bool,
        listener: impl Fn(&MouseDownEvent, &mut Window, &mut App) + 'static,
    ) -> AnyElement {
        let text_color = if danger {
            TAILWIND_RED_400
        } else {
            self.tokens.ui.text
        };
        div()
            .h(px(IDE_AGENT_MENU_ITEM_HEIGHT))
            .w_full()
            .flex()
            .items_center()
            .px_2()
            .gap_2()
            .cursor_pointer()
            .text_color(rgb(text_color))
            .hover(|style| style.bg(rgb(self.tokens.ui.bg_hover)))
            .child(icon)
            .child(div().flex_1().truncate().child(label))
            .on_mouse_down(MouseButton::Left, listener)
            .into_any_element()
    }

    fn render_agent_status_menu_divider(&self) -> AnyElement {
        div()
            .h(px(1.0))
            .my(px(IDE_AGENT_MENU_PADDING_Y))
            .bg(rgb(self.tokens.ui.border))
            .into_any_element()
    }

    fn render_agent_remove_confirm_dialog(&self, cx: &mut Context<Self>) -> AnyElement {
        let tokens = &self.tokens;
        let dialog = dialog_content(tokens)
            .child(
                dialog_header(tokens)
                    .child(dialog_title(
                        tokens,
                        self.labels.agent_remove_confirm_title.clone(),
                    ))
                    .child(dialog_description(
                        tokens,
                        self.labels.agent_remove_confirm_desc.clone(),
                    )),
            )
            .child(
                dialog_footer(tokens)
                    .child(
                        button_with(
                            tokens,
                            self.labels.cancel.clone(),
                            ButtonOptions {
                                variant: ButtonVariant::Outline,
                                size: ButtonSize::Default,
                                radius: ButtonRadius::Md,
                                disabled: false,
                            },
                        )
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(|this, _event, _window, cx| {
                                this.agent_remove_confirm_open = false;
                                cx.notify();
                            }),
                        ),
                    )
                    .child(
                        button_with(
                            tokens,
                            self.labels.agent_remove_confirm_btn.clone(),
                            ButtonOptions {
                                variant: ButtonVariant::Destructive,
                                size: ButtonSize::Default,
                                radius: ButtonRadius::Md,
                                disabled: self.agent_action == Some(AgentActionKind::Remove),
                            },
                        )
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(|this, _event, _window, cx| {
                                this.start_remove_agent(cx);
                            }),
                        ),
                    ),
            );
        self.render_modal_overlay(dialog)
    }

    fn render_agent_opt_in_dialog(&self, cx: &mut Context<Self>) -> AnyElement {
        let tokens = &self.tokens;
        let dialog = div()
            .w(px(IDE_AGENT_OPT_IN_WIDTH))
            .overflow_hidden()
            .rounded(px(tokens.radii.lg))
            .border_1()
            .border_color(rgba(
                (tokens.ui.border << 8) | IDE_AGENT_OPT_IN_BORDER_ALPHA,
            ))
            .bg(rgb(tokens.ui.bg_panel))
            .shadow_lg()
            .child(
                div()
                    .flex()
                    .flex_col()
                    .items_center()
                    .gap(px(IDE_AGENT_OPT_IN_GAP))
                    .px(px(IDE_AGENT_OPT_IN_BODY_PADDING_X))
                    .pt(px(IDE_AGENT_OPT_IN_BODY_PADDING_TOP))
                    .pb(px(IDE_AGENT_OPT_IN_BODY_PADDING_BOTTOM))
                    .child(
                        div()
                            .size(px(IDE_AGENT_OPT_IN_ICON_SIZE))
                            .flex()
                            .items_center()
                            .justify_center()
                            .rounded_full()
                            .border_1()
                            .border_color(rgba(
                                (tokens.ui.accent << 8) | IDE_AGENT_OPT_IN_ACCENT_BORDER_ALPHA,
                            ))
                            .bg(rgba(
                                (tokens.ui.accent << 8) | IDE_AGENT_OPT_IN_ACCENT_BG_ALPHA,
                            ))
                            .child(self.icon(
                                "lucide/bot.svg",
                                IDE_AGENT_OPT_IN_ICON_INNER_SIZE,
                                tokens.ui.accent,
                            )),
                    )
                    .child(
                        div()
                            .text_size(px(tokens.metrics.ui_text_sm))
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(rgb(tokens.ui.text))
                            .text_align(gpui::TextAlign::Center)
                            .child(self.labels.agent_optin_title.clone()),
                    )
                    .child(
                        div()
                            .text_size(px(tokens.metrics.ui_text_xs))
                            .text_color(rgb(tokens.ui.text_muted))
                            .text_align(gpui::TextAlign::Center)
                            .line_height(px(tokens.metrics.ui_text_sm * 1.45))
                            .child(self.labels.agent_optin_desc.clone()),
                    )
                    .child(self.render_agent_opt_in_benefits())
                    .child(self.render_agent_opt_in_remember(cx)),
            )
            .child(
                div()
                    .flex()
                    // Footer hover fills touch the dialog bottom edge; align
                    // the clip to the inner border curve, like browser
                    // DialogContent overflow clipping.
                    .rounded_b(px(rounded_shell_child_radius(tokens.radii.lg)))
                    .overflow_hidden()
                    .border_t_1()
                    .border_color(rgba((tokens.ui.border << 8) | IDE_HOVER_ALPHA))
                    .child(
                        div()
                            .flex_1()
                            .flex()
                            .items_center()
                            .justify_center()
                            .gap_1()
                            .py(px(IDE_AGENT_OPT_IN_ACTION_PADDING_Y))
                            .border_r_1()
                            .border_color(rgba((tokens.ui.border << 8) | IDE_HOVER_ALPHA))
                            .text_size(px(tokens.metrics.ui_text_sm))
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(rgb(tokens.ui.text_muted))
                            .cursor_pointer()
                            .hover(|style| {
                                style
                                    .bg(rgb(tokens.ui.bg_hover))
                                    .text_color(rgb(tokens.ui.text))
                            })
                            .child(self.icon("lucide/folder-sync.svg", 14.0, tokens.ui.text_muted))
                            .child(self.labels.agent_optin_sftp_only.clone())
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(|this, _event, _window, cx| {
                                    this.choose_agent_opt_in(NodeAgentMode::Disabled, cx);
                                    cx.stop_propagation();
                                }),
                            ),
                    )
                    .child(
                        div()
                            .flex_1()
                            .flex()
                            .items_center()
                            .justify_center()
                            .gap_1()
                            .py(px(IDE_AGENT_OPT_IN_ACTION_PADDING_Y))
                            .text_size(px(tokens.metrics.ui_text_sm))
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(rgb(tokens.ui.accent))
                            .cursor_pointer()
                            .hover(|style| {
                                style.bg(rgba(
                                    (tokens.ui.accent << 8) | IDE_AGENT_OPT_IN_ACCENT_BG_ALPHA,
                                ))
                            })
                            .child(self.icon("lucide/bot.svg", 14.0, tokens.ui.accent))
                            .child(self.labels.agent_optin_enable.clone())
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(|this, _event, _window, cx| {
                                    this.choose_agent_opt_in(NodeAgentMode::Enabled, cx);
                                    cx.stop_propagation();
                                }),
                            ),
                    ),
            );
        self.render_modal_overlay(dialog)
    }

    fn render_agent_opt_in_benefits(&self) -> AnyElement {
        let mut benefits = div()
            .w_full()
            .flex()
            .flex_col()
            .gap_1()
            .text_size(px(self.tokens.metrics.ui_text_xs))
            .text_color(rgb(self.tokens.ui.text_muted));
        for text in [
            self.labels.agent_optin_benefit_watch.clone(),
            self.labels.agent_optin_benefit_git.clone(),
            self.labels.agent_optin_benefit_atomic.clone(),
        ] {
            benefits = benefits.child(
                div()
                    .flex()
                    .items_start()
                    .gap_2()
                    .child(self.icon("lucide/check.svg", 12.0, TAILWIND_EMERALD_400))
                    .child(div().flex_1().child(text)),
            );
        }
        benefits.into_any_element()
    }

    fn render_agent_opt_in_remember(&self, cx: &mut Context<Self>) -> AnyElement {
        div()
            .flex()
            .items_center()
            .gap_2()
            .mt_1()
            .cursor_pointer()
            .child(
                div()
                    .size(px(14.0))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(px(self.tokens.radii.xs))
                    .border_1()
                    .border_color(if self.agent_opt_in_remember {
                        rgb(self.tokens.ui.accent)
                    } else {
                        rgb(self.tokens.ui.border)
                    })
                    .bg(if self.agent_opt_in_remember {
                        rgb(self.tokens.ui.accent)
                    } else {
                        rgb(self.tokens.ui.bg_sunken)
                    })
                    .when(self.agent_opt_in_remember, |this| {
                        this.child(self.icon("lucide/check.svg", 10.0, self.tokens.ui.bg))
                    }),
            )
            .child(
                div()
                    .text_size(px(self.tokens.metrics.ui_text_xs))
                    .text_color(rgb(self.tokens.ui.text_muted))
                    .child(self.labels.agent_optin_remember.clone()),
            )
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _event, _window, cx| {
                    this.agent_opt_in_remember = !this.agent_opt_in_remember;
                    cx.stop_propagation();
                    cx.notify();
                }),
            )
            .into_any_element()
    }

    fn choose_agent_opt_in(&mut self, mode: NodeAgentMode, cx: &mut Context<Self>) {
        self.agent_opt_in_open = false;
        if self.agent_opt_in_remember {
            cx.emit(IdeSurfaceEvent::RememberAgentMode(mode));
        } else {
            self.runtime_settings.agent_mode = mode;
            self.fs.set_mode(mode);
        }
        if mode == NodeAgentMode::Enabled {
            self.start_deploy_agent(cx);
        } else {
            self.refresh_agent_status(cx);
        }
        cx.notify();
    }

    fn start_deploy_agent(&mut self, cx: &mut Context<Self>) {
        if matches!(self.agent_action, Some(AgentActionKind::Deploy)) {
            return;
        }
        let Some(node_id) = self.node_id.clone() else {
            return;
        };
        self.agent_action = Some(AgentActionKind::Deploy);
        self.runtime_settings.agent_mode = NodeAgentMode::Enabled;
        self.fs.set_mode(NodeAgentMode::Enabled);
        let fs = self.fs.clone();
        let backend_runtime = self.backend_runtime.clone();
        let generation = self.generation;
        let expected_node_id = node_id.clone();
        cx.notify();
        cx.spawn(async move |weak, cx| {
            let status = backend_runtime
                .spawn(async move { fs.deploy_agent_for_node(node_id).await })
                .await
                .unwrap_or_else(|error| AgentStatus::Failed {
                    reason: error.to_string(),
                });
            let _ = weak.update(cx, |this, cx| {
                if this.generation != generation
                    || this.node_id.as_deref() != Some(expected_node_id.as_str())
                {
                    return;
                }
                this.agent_action = None;
                let _ = status;
                this.start_agent_watch_if_ready(cx);
                this.schedule_next_agent_status_poll(cx);
                cx.notify();
            });
        })
        .detach();
    }

    fn start_remove_agent(&mut self, cx: &mut Context<Self>) {
        if matches!(self.agent_action, Some(AgentActionKind::Remove)) {
            return;
        }
        let Some(node_id) = self.node_id.clone() else {
            return;
        };
        self.agent_action = Some(AgentActionKind::Remove);
        self.agent_remove_confirm_open = false;
        let fs = self.fs.clone();
        let backend_runtime = self.backend_runtime.clone();
        let generation = self.generation;
        let expected_node_id = node_id.clone();
        cx.notify();
        cx.spawn(async move |weak, cx| {
            let result = await_ide_backend(
                backend_runtime.spawn(async move { fs.remove_agent_for_node(node_id).await }),
            )
            .await;
            let _ = weak.update(cx, |this, cx| {
                if this.generation != generation
                    || this.node_id.as_deref() != Some(expected_node_id.as_str())
                {
                    return;
                }
                this.agent_action = None;
                if let Err(error) = result {
                    this.last_error = Some(error.message);
                }
                this.refresh_agent_status(cx);
                cx.notify();
            });
        })
        .detach();
    }

    fn refresh_agent_status(&mut self, cx: &mut Context<Self>) {
        if !matches!(
            self.load_state,
            IdeLoadState::Ready | IdeLoadState::Disconnected
        ) || self.agent_action.is_some()
        {
            return;
        }
        let Some(node_id) = self.node_id.clone() else {
            return;
        };
        self.agent_action = Some(AgentActionKind::Refresh);
        let fs = self.fs.clone();
        let backend_runtime = self.backend_runtime.clone();
        let generation = self.generation;
        let expected_node_id = node_id.clone();
        cx.notify();
        cx.spawn(async move |weak, cx| {
            let _ = backend_runtime
                .spawn(async move { fs.refresh_agent_status(node_id).await })
                .await;
            let _ = weak.update(cx, |this, cx| {
                if this.generation != generation
                    || this.node_id.as_deref() != Some(expected_node_id.as_str())
                {
                    return;
                }
                if this.agent_action == Some(AgentActionKind::Refresh) {
                    this.agent_action = None;
                }
                this.start_agent_watch_if_ready(cx);
                cx.notify();
            });
        })
        .detach();
    }

    fn schedule_next_agent_status_poll(&mut self, cx: &mut Context<Self>) {
        if self.node_id.is_none()
            || !matches!(
                self.load_state,
                IdeLoadState::Ready | IdeLoadState::Disconnected
            )
        {
            return;
        }
        self.agent_poll_generation = self.agent_poll_generation.wrapping_add(1);
        let generation = self.agent_poll_generation;
        let delay = self.agent_poll_delay();
        cx.spawn(async move |weak, cx| {
            Timer::after(delay).await;
            let _ = weak.update(cx, |this, cx| {
                if this.agent_poll_generation != generation {
                    return;
                }
                this.refresh_agent_status(cx);
                this.schedule_next_agent_status_poll(cx);
            });
        })
        .detach();
    }

    fn agent_poll_delay(&self) -> Duration {
        match self.fs.status_for_node(self.node_id.as_deref()) {
            AgentStatus::Deploying => Duration::from_secs(IDE_AGENT_POLL_DEPLOYING_SECS),
            AgentStatus::ManualUploadRequired { .. } | AgentStatus::ManualUpdateRequired { .. } => {
                Duration::from_secs(IDE_AGENT_POLL_MANUAL_SECS)
            }
            _ => Duration::from_secs(IDE_AGENT_POLL_READY_SECS),
        }
    }
}
