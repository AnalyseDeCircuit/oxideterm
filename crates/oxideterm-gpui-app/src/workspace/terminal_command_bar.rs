use super::actions::TerminalBroadcastMenuPlacement;
use super::ime::WorkspaceImeTarget;
use super::*;
use oxideterm_connections::LOCAL_SHELL_PRIVILEGE_CONNECTION_ID;
use oxideterm_gpui_ui::button::{ButtonRadius, IconButtonOptions};
use oxideterm_gpui_ui::context_menu::{ContextMenuActionableStyle, context_menu_event_boundary};
use oxideterm_gpui_ui::modal::rounded_shell_child_radius;
use oxideterm_gpui_ui::text_input::{
    text_caret, text_input_anchor_probe, text_input_value_segments_with_color,
};
use oxideterm_terminal_recording::format_recording_elapsed;

pub(in crate::workspace) mod completion;

const TERMINAL_BROADCAST_MENU_WIDTH: f32 = 260.0;
const TAURI_PRIVILEGE_CHIP_BORDER: u32 = 0xfbbf244d; // Tauri border-amber-400/30
const TAURI_PRIVILEGE_CHIP_BG: u32 = 0xfbbf241a; // Tauri bg-amber-400/10
const TAURI_PRIVILEGE_CHIP_HOVER_BORDER: u32 = 0xfcd34d80; // Tauri hover:border-amber-300/50
const TAURI_PRIVILEGE_CHIP_HOVER_BG: u32 = 0xfbbf2426; // Tauri hover:bg-amber-400/15
const TAURI_PRIVILEGE_CHIP_TEXT: u32 = 0xfde68aff; // Tauri text-amber-200

#[derive(Clone, Debug, Eq, PartialEq)]
struct MatchedPrivilegeCredential {
    connection_id: String,
    credential_id: String,
    label: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct PrivilegePromptHelperState {
    connection_id: String,
    matches: Vec<MatchedPrivilegeCredential>,
}

fn tab_kind_allows_privilege_prompt_helper(tab_kind: &TabKind) -> bool {
    // Tauri passes readVisibleBuffer/sendPrivilegeInput through both SSH and
    // local terminal views. Serial/telnet panes live under LocalTerminal tabs
    // too, so the caller still filters those transport variants separately.
    matches!(tab_kind, TabKind::SshTerminal | TabKind::LocalTerminal)
}

fn privilege_credential_matches_prompt(
    credential: &SavedPrivilegeCredential,
    prompt: &PrivilegePromptMatch,
) -> bool {
    if !credential.enabled {
        return false;
    }
    match prompt {
        PrivilegePromptMatch::Sudo { username, .. } => {
            if !matches!(
                credential.kind,
                PrivilegeCredentialKind::SudoPassword | PrivilegeCredentialKind::CustomPrompt
            ) {
                return false;
            }
            if credential.kind == PrivilegeCredentialKind::CustomPrompt {
                return privilege_prompt_matches_custom_patterns(
                    prompt,
                    &credential.prompt_patterns,
                );
            }
            username.as_ref().is_none_or(|prompt_username| {
                credential
                    .username_hint
                    .as_ref()
                    .is_none_or(|hint| prompt_username == hint)
            })
        }
        PrivilegePromptMatch::Su { .. } => {
            if !matches!(
                credential.kind,
                PrivilegeCredentialKind::SuPassword | PrivilegeCredentialKind::CustomPrompt
            ) {
                return false;
            }
            credential.kind != PrivilegeCredentialKind::CustomPrompt
                || privilege_prompt_matches_custom_patterns(prompt, &credential.prompt_patterns)
        }
        PrivilegePromptMatch::Custom { credential_id, .. } => credential.id == *credential_id,
    }
}

fn privilege_prompt_matches_custom_patterns(
    prompt: &PrivilegePromptMatch,
    patterns: &[String],
) -> bool {
    let prompt_text = match prompt {
        PrivilegePromptMatch::Sudo { prompt_text, .. }
        | PrivilegePromptMatch::Su { prompt_text, .. }
        | PrivilegePromptMatch::Custom { prompt_text, .. } => prompt_text,
    }
    .to_ascii_lowercase();
    patterns
        .iter()
        .map(|pattern| pattern.trim().to_ascii_lowercase())
        .any(|pattern| !pattern.is_empty() && prompt_text.contains(&pattern))
}

fn build_privilege_prompt_helper_state(
    connection_id: String,
    credentials: &[SavedPrivilegeCredential],
    visible_text: &str,
) -> Option<PrivilegePromptHelperState> {
    let prompt = detect_privilege_prompt(visible_text)?;
    let matches = credentials
        .iter()
        .filter(|credential| privilege_credential_matches_prompt(credential, &prompt))
        .map(|credential| MatchedPrivilegeCredential {
            connection_id: connection_id.clone(),
            credential_id: credential.id.clone(),
            label: credential.label.clone(),
        })
        .collect();
    Some(PrivilegePromptHelperState {
        connection_id,
        matches,
    })
}

impl WorkspaceApp {
    fn saved_connection_id_for_node_snapshot(
        &self,
        node_id: &NodeId,
        node: Option<&WorkspaceSshNode>,
    ) -> Option<String> {
        let config = node.map(|node| node.config.clone()).or_else(|| {
            self.node_runtime_store
                .snapshot(node_id)
                .map(|snapshot| snapshot.config)
        })?;
        let title = node.map(|node| node.title.as_str());
        let candidates = self
            .connection_store
            .connections()
            .iter()
            .filter(|connection| {
                connection.host == config.host
                    && connection.port == config.port
                    && connection.username == config.username
            })
            .collect::<Vec<_>>();
        if let Some(title) = title
            && let Some(connection) = candidates
                .iter()
                .copied()
                .find(|connection| connection.name == title)
        {
            return Some(connection.id.clone());
        }
        // Use a config match as a last resort only when it is unique. Privilege
        // helper credentials are secrets; ambiguous host aliases must not pick
        // a saved connection by accident.
        match candidates.as_slice() {
            [connection] => Some(connection.id.clone()),
            _ => None,
        }
    }

    fn terminal_command_action_button(
        &self,
        icon: LucideIcon,
        icon_color: Rgba,
        disabled: bool,
        background: Option<Rgba>,
        listener: impl Fn(&mut Self, &MouseDownEvent, &mut Window, &mut Context<Self>) + 'static,
        cx: &mut Context<Self>,
    ) -> gpui::Div {
        // Tauri TerminalCommandBarActions uses a shared h-6/w-6 rounded-md
        // button for split, broadcast, recording, and cast controls. Keep the
        // geometry local to the terminal bar while routing activation through
        // the workspace button guard shared with FileManager/SFTP actions.
        self.workspace_icon_action_button(
            icon,
            14.0,
            icon_color,
            IconButtonOptions {
                disabled,
                background,
                hover_background: Some(rgb(self.tokens.ui.bg_hover)),
                ..IconButtonOptions::opaque_toolbar(24.0, ButtonRadius::Md)
            },
            listener,
            cx,
        )
    }

    fn active_privilege_connection_id(&self) -> Option<String> {
        if self
            .active_tab()
            .is_some_and(|tab| tab.kind == TabKind::LocalTerminal)
            && !self.active_tab_has_serial_terminal()
        {
            // Local shell sudo/su prompts have no SavedConnection owner. Use a
            // dedicated store scope so secrets are never confused with SSH
            // connection credentials.
            return Some(LOCAL_SHELL_PRIVILEGE_CONNECTION_ID.to_string());
        }

        let session_id = self.active_terminal_session_id()?;
        if let Some(connection_id) = self.terminal_privilege_connection_ids.get(&session_id) {
            return Some(connection_id.clone());
        }
        let node_id = self.terminal_ssh_nodes.get(&session_id)?;
        let node = self.ssh_nodes.get(node_id);
        // Privilege credentials are stored on SavedConnection metadata, not on
        // transient SSH transport handles. Resolve the owner from every native
        // session-tree mirror before giving up: restored/expanded nodes may
        // have their origin in NodeRuntimeStore even when the UI node snapshot
        // was created before the saved id was attached.
        node.and_then(|node| node.saved_connection_id.clone())
            .or_else(|| {
                self.node_runtime_store
                    .snapshot(node_id)
                    .and_then(|snapshot| snapshot.origin.saved_connection_id().map(str::to_string))
            })
            .or_else(|| {
                self.saved_ssh_nodes
                    .iter()
                    .find_map(|(saved_connection_id, saved_node_id)| {
                        (saved_node_id == node_id).then(|| saved_connection_id.clone())
                    })
            })
            .or_else(|| self.saved_connection_id_for_node_snapshot(node_id, node))
    }

    fn active_privilege_prompt_state(
        &self,
        cx: &mut Context<Self>,
    ) -> Option<PrivilegePromptHelperState> {
        let active_tab = self.active_tab()?;
        if !tab_kind_allows_privilege_prompt_helper(&active_tab.kind) {
            return None;
        }
        let visible_text = self.active_pane()?.read(cx).visible_text_snapshot();
        let _prompt = detect_privilege_prompt(&visible_text)?;
        let connection_id = self.active_privilege_connection_id()?;
        // Tauri keeps the prompt state alive even when credential metadata
        // cannot be loaded; the chip then becomes a management affordance. Do
        // not let a missing credential row or transient keychain/config error
        // suppress the detected sudo/su prompt.
        let credentials = self
            .connection_store
            .list_privilege_credentials(&connection_id)
            .unwrap_or_default();
        build_privilege_prompt_helper_state(connection_id, &credentials, &visible_text)
    }

    pub(in crate::workspace) fn active_privilege_prompt_helper_should_refresh(
        &self,
        cx: &mut Context<Self>,
    ) -> bool {
        if !self.settings_store.settings().terminal.command_bar.enabled {
            return false;
        }
        self.active_privilege_prompt_state(cx).is_some()
    }

    fn fill_privilege_prompt_match(
        &mut self,
        matched: MatchedPrivilegeCredential,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let secret = match self
            .connection_store
            .get_privilege_credential_secret(&matched.connection_id, &matched.credential_id)
        {
            Ok(secret) => secret,
            Err(error) => {
                self.push_command_palette_toast(
                    self.i18n.t("terminal.privilege_helper.load_failed"),
                    Some(error.to_string()),
                    TerminalNoticeVariant::Error,
                );
                cx.notify();
                return;
            }
        };
        // The newline-bearing buffer is the only owned cleartext copy in the
        // GPUI layer. It is zeroized after the PTY write attempt, matching the
        // Tauri click-only secret handoff without involving command history.
        let secret_line = zeroize::Zeroizing::new(format!("{}\n", secret.expose_secret()));
        let sent = self.active_pane().is_some_and(|pane| {
            pane.update(cx, |pane, cx| {
                pane.send_privilege_secret_input_bytes(secret_line.as_bytes(), cx)
            })
        });
        if !sent {
            self.push_command_palette_toast(
                self.i18n.t("terminal.privilege_helper.send_failed"),
                None,
                TerminalNoticeVariant::Error,
            );
        }
        self.focus_active_pane(window, cx);
        cx.notify();
    }

    fn manage_active_privilege_prompt_credentials(
        &mut self,
        connection_id: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if connection_id == LOCAL_SHELL_PRIVILEGE_CONNECTION_ID {
            self.settings_page.set_active_tab(SettingsTab::Local);
            self.open_settings(window, cx);
            cx.notify();
            return;
        }
        if self.connection_store.get(&connection_id).is_none() {
            self.push_command_palette_toast(
                self.i18n.t("terminal.privilege_helper.load_failed"),
                Some(format!("Connection not found: {connection_id}")),
                TerminalNoticeVariant::Error,
            );
            cx.notify();
            return;
        }
        // The no-credential prompt state is a management affordance only. It
        // opens the same saved-connection editor as Tauri and never reads a
        // keychain item until the user explicitly saves and later clicks Fill.
        self.open_saved_connection_editor(&connection_id, None, window, cx);
    }

    pub(in crate::workspace) fn render_terminal_surface(
        &self,
        root_pane: &PaneNode,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let terminal = self.render_pane_tree(root_pane, cx);
        let recording_status = self.active_terminal_recording_status(cx);
        let recording_active = recording_status.state != TerminalRecordingState::Idle;
        if !self.settings_store.settings().terminal.command_bar.enabled {
            return div()
                .size_full()
                .relative()
                .child(terminal)
                .when(recording_active, |surface| {
                    surface.child(self.render_terminal_recording_controls(recording_status, cx))
                })
                .into_any_element();
        }

        div()
            .size_full()
            .flex()
            .flex_col()
            .child(
                div()
                    .relative()
                    .flex_1()
                    .min_h(px(0.0))
                    .child(terminal)
                    .when(recording_active, |surface| {
                        surface.child(self.render_terminal_recording_controls(recording_status, cx))
                    }),
            )
            .child(self.render_terminal_command_bar(cx))
            .into_any_element()
    }

    fn render_terminal_command_bar(&self, cx: &mut Context<Self>) -> AnyElement {
        const COMMAND_BAR_BG_ALPHA: u32 = 0xf2; // Tauri bg-theme-bg/95
        const COMMAND_BAR_BORDER_ALPHA: u32 = 0xb3; // Tauri border-theme-border/70
        const COMMAND_BAR_INPUT_BORDER_ALPHA: u32 = 0x73; // Tauri border-theme-border/45
        const COMMAND_BAR_FOCUSED_BORDER_ALPHA: u32 = 0x73; // Tauri border-theme-accent/45

        let theme = self.tokens.ui;
        let target = WorkspaceImeTarget::TerminalCommandBar;
        let workspace = cx.entity();
        let focused = self.terminal_command_bar_focused;
        let marked_text = self.marked_text_for_target(target);
        let selected_range = self.ime_selected_range_for_target(target);
        let command_is_empty = self.terminal_command_bar_draft.is_empty();
        let command_suggestions = if focused {
            self.terminal_command_bar_suggestions(false, cx)
        } else {
            Vec::new()
        };
        let ghost_text = self.terminal_command_ghost_text(&command_suggestions);
        let showing_placeholder = command_is_empty && marked_text.is_none();
        let command_text = if showing_placeholder {
            self.i18n.t("terminal.command_bar.command_placeholder")
        } else {
            self.terminal_command_bar_draft.clone()
        };
        let input_range = selected_range
            .clone()
            .filter(|_| focused && !command_is_empty && marked_text.is_none());
        let selection_range = input_range.clone().filter(|range| range.start < range.end);
        let caret_offset = input_range
            .as_ref()
            .filter(|range| range.start == range.end)
            .map(|range| range.start);
        let shows_selection = selection_range.is_some();
        let shows_positioned_caret = caret_offset.is_some() && !shows_selection;
        // The visible chip and completion providers share Tauri's target-label
        // inference so local shells that are currently inside SSH show the
        // remote identity consistently in both places.
        let target_label = self.terminal_command_active_target_label(cx);
        let active_pane_id = self.active_pane_id();
        let is_local_terminal = self
            .active_tab()
            .is_some_and(|tab| tab.kind == TabKind::LocalTerminal);
        let can_split = self.active_tab().is_some_and(|tab| {
            tab.kind == TabKind::LocalTerminal
                && !self.active_tab_has_serial_terminal()
                && tab
                    .root_pane
                    .as_ref()
                    .is_some_and(|root| root.pane_count() < MAX_PANES_PER_TAB)
        });
        let broadcast_targets =
            self.terminal_broadcast_target_panes(active_pane_id.unwrap_or(PaneId(0)));
        let broadcast_label = if self.terminal_broadcast_enabled {
            if self.terminal_broadcast_targets.is_empty() {
                self.i18n.t("terminal.command_bar.all_targets")
            } else {
                format!("{}", broadcast_targets.len())
            }
        } else {
            String::new()
        };
        let quick_commands_enabled = self
            .settings_store
            .settings()
            .terminal
            .command_bar
            .quick_commands_enabled;
        let recording_status = self.active_terminal_recording_status(cx);
        let recording_active = recording_status.state != TerminalRecordingState::Idle;
        let privilege_prompt_state = self.active_privilege_prompt_state(cx);

        let bar = div()
            .relative()
            .flex_none()
            .border_t_1()
            .border_color(rgba((theme.border << 8) | COMMAND_BAR_BORDER_ALPHA))
            .bg(rgba((theme.bg << 8) | COMMAND_BAR_BG_ALPHA))
            .px(px(12.0))
            .py(px(4.0))
            .shadow_lg()
            .when(
                focused
                    && self.terminal_command_suggestions_open
                    && !command_suggestions.is_empty(),
                |bar| bar.child(self.render_terminal_command_suggestions(&command_suggestions, cx)),
            )
            .when(
                quick_commands_enabled && self.terminal_quick_commands_open,
                |bar| {
                    // Tauri renders QuickCommandsPopover as a child of the relative
                    // TerminalCommandBar (`absolute bottom-full right-3`). Keep the
                    // native popover on the same local coordinate owner; routing it
                    // through the root backdrop makes the existing bottom/right
                    // placement resolve against the wrong box.
                    bar.child(self.render_terminal_quick_commands_popover(cx))
                },
            )
            .child(
                div()
                    .min_h(px(24.0))
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap(px(8.0))
                    .child(
                        div()
                            .truncate()
                            .text_size(px(11.0))
                            .text_color(rgb(theme.text_muted))
                            .child(target_label),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(4.0))
                            .when(
                                self.terminal_broadcast_enabled && !broadcast_label.is_empty(),
                                |actions| {
                                    actions.child(
                                        div()
                                            .h(px(20.0))
                                            .px(px(6.0))
                                            .flex()
                                            .items_center()
                                            .gap(px(4.0))
                                            .rounded(px(self.tokens.radii.md))
                                            .border_1()
                                            .border_color(rgba(0xf973164d))
                                            .bg(rgba(0xf973161a))
                                            .text_size(px(11.0))
                                            .text_color(rgba(0xfdba74ff))
                                            .child(Self::render_lucide_icon(
                                                LucideIcon::Radio,
                                                12.0,
                                                rgba(0xfdba74ff),
                                            ))
                                            .child(broadcast_label),
                                    )
                                },
                            )
                            .when_some(privilege_prompt_state, |mut actions, state| {
                                for (index, matched) in state.matches.iter().cloned().enumerate() {
                                    let tooltip_id = format!("terminal-privilege-helper-fill-{index}");
                                    let title = self.i18n_replace(
                                        "terminal.privilege_helper.fill_title",
                                        &[("label", matched.label.clone())],
                                    );
                                    actions = actions.child(
                                        div()
                                            .h(px(20.0))
                                            .px(px(6.0))
                                            .flex()
                                            .items_center()
                                            .gap(px(4.0))
                                            .rounded(px(self.tokens.radii.md))
                                            .border_1()
                                            .border_color(rgba(TAURI_PRIVILEGE_CHIP_BORDER))
                                            .bg(rgba(TAURI_PRIVILEGE_CHIP_BG))
                                            .text_size(px(11.0))
                                            .text_color(rgba(TAURI_PRIVILEGE_CHIP_TEXT))
                                            .id(("terminal-privilege-helper-fill", index))
                                            .on_mouse_move({
                                                let title = title.clone();
                                                let tooltip_id = tooltip_id.clone();
                                                cx.listener(move |this, event: &MouseMoveEvent, _window, cx| {
                                                    this.queue_workspace_tooltip(
                                                        tooltip_id.clone(),
                                                        title.clone(),
                                                        f32::from(event.position.x) + 12.0,
                                                        f32::from(event.position.y) + 16.0,
                                                        cx,
                                                    );
                                                })
                                            })
                                            .on_hover({
                                                let tooltip_id = tooltip_id.clone();
                                                cx.listener(move |this, hovered: &bool, _window, cx| {
                                                    if !*hovered {
                                                        this.clear_workspace_tooltip(&tooltip_id, cx);
                                                    }
                                                })
                                            })
                                            .hover(|style| {
                                                style
                                                    .border_color(rgba(TAURI_PRIVILEGE_CHIP_HOVER_BORDER))
                                                    .bg(rgba(TAURI_PRIVILEGE_CHIP_HOVER_BG))
                                            })
                                            .on_mouse_down(
                                                MouseButton::Left,
                                                cx.listener(move |this, _event, window, cx| {
                                                    this.fill_privilege_prompt_match(matched.clone(), window, cx);
                                                    cx.stop_propagation();
                                                }),
                                            )
                                            .child(Self::render_lucide_icon(
                                                LucideIcon::KeyRound,
                                                12.0,
                                                rgba(TAURI_PRIVILEGE_CHIP_TEXT),
                                            ))
                                            .child(self.i18n.t("terminal.privilege_helper.fill")),
                                    );
                                }
                                if state.matches.is_empty() {
                                    let title = self.i18n.t("terminal.privilege_helper.manage_title");
                                    let connection_id = state.connection_id.clone();
                                    actions = actions.child(
                                        div()
                                            .h(px(20.0))
                                            .px(px(6.0))
                                            .flex()
                                            .items_center()
                                            .gap(px(4.0))
                                            .rounded(px(self.tokens.radii.md))
                                            .border_1()
                                            .border_color(rgba(TAURI_PRIVILEGE_CHIP_BORDER))
                                            .bg(rgba(TAURI_PRIVILEGE_CHIP_BG))
                                            .text_size(px(11.0))
                                            .text_color(rgba(TAURI_PRIVILEGE_CHIP_TEXT))
                                            .id("terminal-privilege-helper-manage")
                                            .on_mouse_move({
                                                let title = title.clone();
                                                cx.listener(move |this, event: &MouseMoveEvent, _window, cx| {
                                                    this.queue_workspace_tooltip(
                                                        "terminal-privilege-helper-manage",
                                                        title.clone(),
                                                        f32::from(event.position.x) + 12.0,
                                                        f32::from(event.position.y) + 16.0,
                                                        cx,
                                                    );
                                                })
                                            })
                                            .on_hover(cx.listener(|this, hovered: &bool, _window, cx| {
                                                if !*hovered {
                                                    this.clear_workspace_tooltip(
                                                        "terminal-privilege-helper-manage",
                                                        cx,
                                                    );
                                                }
                                            }))
                                            .hover(|style| {
                                                style
                                                    .border_color(rgba(TAURI_PRIVILEGE_CHIP_HOVER_BORDER))
                                                    .bg(rgba(TAURI_PRIVILEGE_CHIP_HOVER_BG))
                                            })
                                            .on_mouse_down(
                                                MouseButton::Left,
                                                cx.listener(move |this, _event, window, cx| {
                                                    this.manage_active_privilege_prompt_credentials(
                                                        connection_id.clone(),
                                                        window,
                                                        cx,
                                                    );
                                                    cx.stop_propagation();
                                                }),
                                            )
                                            .child(Self::render_lucide_icon(
                                                LucideIcon::KeyRound,
                                                12.0,
                                                rgba(TAURI_PRIVILEGE_CHIP_TEXT),
                                            ))
                                            .child(self.i18n.t("terminal.privilege_helper.manage")),
                                    );
                                }
                                actions
                            })
                            .when(is_local_terminal, |actions| {
                                actions
                                    .child(self.terminal_command_action_button(
                                        LucideIcon::SplitSquareHorizontal,
                                        rgb(theme.text_muted),
                                        !can_split,
                                        None,
                                        |this, _event, window, cx| {
                                            this.split_active_pane(
                                                SplitDirection::Horizontal,
                                                window,
                                                cx,
                                            );
                                            cx.stop_propagation();
                                        },
                                        cx,
                                    ))
                                    .child(self.terminal_command_action_button(
                                        LucideIcon::SplitSquareVertical,
                                        rgb(theme.text_muted),
                                        !can_split,
                                        None,
                                        |this, _event, window, cx| {
                                            this.split_active_pane(
                                                SplitDirection::Vertical,
                                                window,
                                                cx,
                                            );
                                            cx.stop_propagation();
                                        },
                                        cx,
                                    ))
                            })
                            .child(select_anchor_probe(
                                SelectAnchorId::TerminalBroadcastMenu,
                                self.terminal_command_action_button(
                                    LucideIcon::Radio,
                                    if self.terminal_broadcast_enabled {
                                        rgba(0xfb923cff)
                                    } else {
                                        rgb(theme.text_muted)
                                    },
                                    false,
                                    Some(if self.terminal_broadcast_enabled {
                                        rgba(0xf9731626)
                                    } else {
                                        rgba((theme.bg_hover << 8) | 0x00)
                                    }),
                                    |this, _event, _window, cx| {
                                        this.toggle_terminal_broadcast_menu();
                                        cx.stop_propagation();
                                        cx.notify();
                                    },
                                    cx,
                                )
                                .relative(),
                                {
                                    let workspace = workspace.clone();
                                    move |anchor, _window, cx| {
                                        let _ = workspace.update(cx, |this, cx| {
                                            this.update_select_anchor(anchor, cx);
                                        });
                                    }
                                },
                            ))
                            .when(recording_active, |actions| {
                                actions.child(
                                    div()
                                        .h(px(20.0))
                                        .px(px(6.0))
                                        .flex()
                                        .items_center()
                                        .gap(px(4.0))
                                        .rounded(px(self.tokens.radii.md))
                                        .border_1()
                                        .border_color(rgba(0xef44444d))
                                        .bg(rgba(0xef44441a))
                                        .text_size(px(11.0))
                                        .text_color(rgba(0xfca5a5ff))
                                        .child(Self::render_lucide_icon(
                                            LucideIcon::Circle,
                                            10.0,
                                            rgba(0xfca5a5ff),
                                        ))
                                        .child(format_recording_elapsed(recording_status.elapsed)),
                                )
                            })
                            .child(self.terminal_command_action_button(
                                match recording_status.state {
                                    TerminalRecordingState::Paused => LucideIcon::Play,
                                    _ => LucideIcon::Circle,
                                },
                                if recording_active {
                                    rgba(0xf87171ff)
                                } else {
                                    rgb(theme.text_muted)
                                },
                                false,
                                Some(if recording_active {
                                    rgba(0xef444426)
                                } else {
                                    rgba(0x00000000)
                                }),
                                move |this, _event, _window, cx| {
                                    match recording_status.state {
                                        TerminalRecordingState::Idle => {
                                            this.start_active_terminal_recording(cx)
                                        }
                                        TerminalRecordingState::Recording => {
                                            this.pause_active_terminal_recording(cx)
                                        }
                                        TerminalRecordingState::Paused => {
                                            this.resume_active_terminal_recording(cx)
                                        }
                                    }
                                    cx.stop_propagation();
                                },
                                cx,
                            ))
                            .when(recording_active, |actions| {
                                actions
                                    .child(self.terminal_command_action_button(
                                        LucideIcon::Square,
                                        rgba(0xf87171ff),
                                        false,
                                        None,
                                        |this, _event, _window, cx| {
                                            this.stop_active_terminal_recording(cx);
                                            cx.stop_propagation();
                                        },
                                        cx,
                                    ))
                                    .child(self.terminal_command_action_button(
                                        LucideIcon::Trash2,
                                        rgba(0xf87171ff),
                                        false,
                                        None,
                                        |this, _event, _window, cx| {
                                            this.discard_active_terminal_recording(cx);
                                            cx.stop_propagation();
                                        },
                                        cx,
                                    ))
                            })
                            .child(self.terminal_command_action_button(
                                LucideIcon::FilePlay,
                                rgb(theme.text_muted),
                                false,
                                None,
                                |this, _event, window, cx| {
                                    this.open_terminal_cast_file(window, cx);
                                    cx.stop_propagation();
                                },
                                cx,
                            )),
                    ),
            )
            .child(
                div()
                    .mt(px(2.0))
                    .pt(px(4.0))
                    .border_t_1()
                    .border_color(if focused {
                        rgba((theme.accent << 8) | COMMAND_BAR_FOCUSED_BORDER_ALPHA)
                    } else {
                        rgba((theme.border << 8) | COMMAND_BAR_INPUT_BORDER_ALPHA)
                    })
                    .flex()
                    .items_center()
                    .gap(px(8.0))
                    .child(
                        div()
                            .flex_1()
                            .min_w(px(0.0))
                            .flex()
                            .items_center()
                            .gap(px(8.0))
                            .cursor_text()
                            // Tauri only focuses the command textarea when the
                            // row background or textarea area receives the
                            // pointer. Keep the quick-command button outside
                            // this hit region so its click cannot be captured
                            // by IME selection before the toggle handler runs.
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(
                                    move |this, event: &gpui::MouseDownEvent, window, cx| {
                                        this.terminal_command_bar_focused = true;
                                        this.ime_marked_text = None;
                                        window.focus(&this.focus_handle);
                                        this.begin_ime_selection_from_mouse_down(
                                            WorkspaceImeTarget::TerminalCommandBar,
                                            event,
                                            window,
                                            cx,
                                        );
                                        cx.stop_propagation();
                                    },
                                ),
                            )
                            .on_mouse_move(cx.listener(
                                |this, event: &gpui::MouseMoveEvent, window, cx| {
                                    this.update_ime_selection_drag_from_mouse_move(
                                        event, window, cx,
                                    );
                                },
                            ))
                            .child(Self::render_lucide_icon(
                                LucideIcon::ChevronRight,
                                16.0,
                                rgb(theme.text_muted),
                            ))
                            .child(text_input_anchor_probe(
                                target.anchor_id(),
                                div()
                                    .h(px(24.0))
                                    .flex_1()
                                    .flex()
                                    .items_center()
                                    .overflow_hidden()
                                    .text_size(px(13.0))
                                    .font_family(settings_mono_font_family(
                                        self.settings_store.settings(),
                                    ))
                                    .text_color(if showing_placeholder {
                                        rgb(theme.text_muted)
                                    } else {
                                        rgb(theme.text)
                                    })
                                    .when(focused && showing_placeholder, |input| {
                                        input.child(text_caret(
                                            &self.tokens,
                                            self.new_connection_caret_visible,
                                        ))
                                    })
                                    // Tauri uses a real textarea, so the painted caret
                                    // follows selectionStart instead of always sitting
                                    // at the end of the value. Keep native rendering
                                    // tied to the shared IME range for click/arrow parity.
                                    .child(if showing_placeholder {
                                        div().child(command_text).into_any_element()
                                    } else {
                                        text_input_value_segments_with_color(
                                            &self.tokens,
                                            &command_text,
                                            false,
                                            selection_range,
                                            caret_offset,
                                            self.new_connection_caret_visible,
                                            Some(theme.text),
                                        )
                                        .into_any_element()
                                    })
                                    .when_some(marked_text, |input, marked| {
                                        input.child(
                                            div()
                                                .underline()
                                                .text_color(rgb(theme.text))
                                                .child(marked.to_string()),
                                        )
                                    })
                                    .when(
                                        focused
                                            && !showing_placeholder
                                            && !shows_selection
                                            && !shows_positioned_caret,
                                        |input| {
                                            input.child(text_caret(
                                                &self.tokens,
                                                self.new_connection_caret_visible,
                                            ))
                                        },
                                    )
                                    .when_some(ghost_text, |input, ghost| {
                                        input.child(
                                            div()
                                                .text_color(rgba((theme.text_muted << 8) | 0x99))
                                                .child(ghost),
                                        )
                                    }),
                                {
                                    let workspace = workspace.clone();
                                    move |anchor, _window, cx| {
                                        let _ = workspace.update(cx, |this, cx| {
                                            this.update_text_input_anchor(anchor, cx);
                                        });
                                    }
                                },
                            )),
                    )
                    .when(quick_commands_enabled, |input_row| {
                        input_row.child(
                            div()
                                .size(px(24.0))
                                .flex()
                                .items_center()
                                .justify_center()
                                .rounded(px(self.tokens.radii.md))
                                .cursor_pointer()
                                .bg(if self.terminal_quick_commands_open {
                                    rgba((theme.accent << 8) | 0x1a)
                                } else {
                                    rgba(0x00000000)
                                })
                                .text_color(if self.terminal_quick_commands_open {
                                    rgb(theme.accent)
                                } else {
                                    rgb(theme.text_muted)
                                })
                                .hover(move |style| style.bg(rgb(theme.bg_hover)))
                                .on_mouse_down(
                                    MouseButton::Left,
                                    cx.listener(|this, _event, _window, cx| {
                                        this.terminal_quick_commands_open =
                                            !this.terminal_quick_commands_open;
                                        this.dismiss_terminal_broadcast_menu();
                                        if !this.terminal_quick_commands_open {
                                            this.close_terminal_quick_commands_popover();
                                        }
                                        cx.stop_propagation();
                                        cx.notify();
                                    }),
                                )
                                .child(Self::render_lucide_icon(
                                    LucideIcon::Zap,
                                    14.0,
                                    if self.terminal_quick_commands_open {
                                        rgb(theme.accent)
                                    } else {
                                        rgb(theme.text_muted)
                                    },
                                )),
                        )
                    }),
            );
        select_anchor_probe(
            SelectAnchorId::TerminalCommandBar,
            bar,
            move |anchor, _window, cx| {
                let _ = workspace.update(cx, |this, cx| {
                    this.update_select_anchor(anchor, cx);
                });
            },
        )
        .into_any_element()
    }

    pub(in crate::workspace) fn render_terminal_quick_commands_popover(
        &self,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        self.render_quick_commands_popover(cx)
    }

    pub(in crate::workspace) fn render_terminal_broadcast_menu(
        &self,
        placement: TerminalBroadcastMenuPlacement,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let entries = self.terminal_broadcast_entries();
        let active_pane_id = self.active_pane_id();
        let selectable = entries
            .iter()
            .filter(|(pane_id, _, _)| Some(*pane_id) != active_pane_id)
            .map(|(pane_id, _, _)| *pane_id)
            .collect::<Vec<_>>();
        let all_selected = !selectable.is_empty()
            && selectable
                .iter()
                .all(|pane_id| self.terminal_broadcast_targets.contains(pane_id));
        let anchor_left = self
            .select_anchors
            .get(&SelectAnchorId::TerminalBroadcastMenu)
            .map(|anchor| {
                // Tauri uses Radix DropdownMenuContent with `align="end"`.
                // Align to the trigger instead of the workspace root, because
                // the AI sidebar changes the root width but not the terminal
                // command-bar button's visual anchor.
                terminal_broadcast_menu_left_for_trigger_right(f32::from(anchor.bounds.right()))
            });

        let mut menu = context_menu_event_boundary({
            let menu = div()
                .absolute()
                .w(px(TERMINAL_BROADCAST_MENU_WIDTH))
                .max_h(px(320.0))
                .overflow_hidden()
                .rounded(px(self.tokens.radii.lg))
                .border_1()
                .border_color(rgb(theme.border))
                .bg(rgba((theme.bg_elevated << 8) | 0xf2))
                .shadow_lg()
                .p(px(6.0))
                .text_size(px(12.0));
            if let Some(left) = anchor_left {
                menu.left(px(left))
            } else {
                menu.right(px(12.0))
            }
        })
        .child(
            div()
                .px(px(6.0))
                .py(px(4.0))
                .text_size(px(11.0))
                .text_color(rgb(theme.text_muted))
                .child(self.i18n.t("terminal.broadcast.select_targets")),
        );
        menu = match placement {
            TerminalBroadcastMenuPlacement::Bottom(offset) => menu.bottom(px(offset)),
            TerminalBroadcastMenuPlacement::Top(offset) => menu.top(px(offset)),
        };

        if entries.len() <= 1 {
            menu = menu.child(
                div()
                    .px(px(8.0))
                    .py(px(12.0))
                    .text_align(gpui::TextAlign::Center)
                    .text_color(rgb(theme.text_muted))
                    .child(self.i18n.t("terminal.broadcast.no_targets")),
            );
        } else {
            for (pane_id, label, kind) in entries {
                let is_current = Some(pane_id) == active_pane_id;
                let checked = self.terminal_broadcast_targets.contains(&pane_id);
                let badge = match kind {
                    TabKind::LocalTerminal => self.i18n.t("terminal.typeLocal"),
                    TabKind::SshTerminal => self.i18n.t("terminal.typeSsh"),
                    _ => String::new(),
                };
                let row_color = if is_current {
                    rgb(theme.text_muted)
                } else {
                    rgb(theme.text)
                };
                let row = div()
                    .h(px(30.0))
                    .flex()
                    .items_center()
                    .gap(px(8.0))
                    .px(px(8.0))
                    .rounded(px(self.tokens.radii.md))
                    .text_color(row_color)
                    .child(if checked {
                        Self::render_lucide_icon(LucideIcon::Check, 12.0, rgba(0xfb923cff))
                    } else if is_current {
                        div()
                            .size(px(12.0))
                            .rounded_full()
                            .bg(rgba(0xfb923cff))
                            .into_any_element()
                    } else {
                        div().size(px(12.0)).into_any_element()
                    })
                    .child(div().flex_1().truncate().child(label))
                    .when(!badge.is_empty(), |row| {
                        row.child(
                            div()
                                .px(px(5.0))
                                .py(px(1.0))
                                .rounded(px(self.tokens.radii.md))
                                .text_size(px(10.0))
                                .text_color(rgb(theme.text_muted))
                                .bg(rgba((theme.bg_panel << 8) | 0x99))
                                .child(badge),
                        )
                    })
                    .when(is_current, |row| {
                        row.child(
                            div()
                                .px(px(5.0))
                                .py(px(1.0))
                                .rounded(px(self.tokens.radii.md))
                                .text_size(px(10.0))
                                .text_color(rgba(0xfb923cff))
                                .bg(rgba(0xf9731626))
                                .child(self.i18n.t("terminal.broadcast.current")),
                        )
                    });
                // Broadcast rows are checkbox-style menu items. Keep current
                // pane disabled through the shared menu action guard.
                let row = self.render_terminal_broadcast_menu_action(
                    row,
                    is_current,
                    false,
                    Some(rgb(theme.bg_hover)),
                    cx.listener(move |this, _event, _window, _cx| {
                        if this.terminal_broadcast_targets.remove(&pane_id) {
                            if this.terminal_broadcast_targets.is_empty() {
                                this.terminal_broadcast_enabled = false;
                            }
                        } else {
                            this.terminal_broadcast_targets.insert(pane_id);
                            this.terminal_broadcast_enabled = true;
                        }
                        this.keep_terminal_broadcast_menu_open();
                    }),
                    cx,
                );
                menu = menu.child(row);
            }

            let select_all_disabled = selectable.is_empty();
            let select_all_label = div()
                .text_size(px(11.0))
                .text_color(rgb(theme.text_muted))
                .child(if all_selected {
                    self.i18n.t("terminal.broadcast.deselect_all")
                } else {
                    self.i18n.t("terminal.broadcast.select_all")
                });
            menu = menu.child(
                div()
                    .mt(px(4.0))
                    .pt(px(6.0))
                    .border_t_1()
                    .border_color(rgba((theme.border << 8) | 0x99))
                    .flex()
                    .items_center()
                    .justify_between()
                    .px(px(6.0))
                    .child(self.workspace_context_menu_persistent_styled_action(
                        select_all_label,
                        select_all_disabled,
                        false,
                        ContextMenuActionableStyle {
                            hover_background: None,
                            hover_text_color: Some(rgb(theme.accent)),
                        },
                        move |this, _event, _window, _cx| {
                            if all_selected {
                                this.terminal_broadcast_enabled = false;
                                this.terminal_broadcast_targets.clear();
                            } else {
                                this.terminal_broadcast_targets =
                                    selectable.iter().copied().collect();
                                this.terminal_broadcast_enabled =
                                    !this.terminal_broadcast_targets.is_empty();
                            }
                            this.keep_terminal_broadcast_menu_open();
                        },
                        cx,
                    ))
                    .when(self.terminal_broadcast_enabled, |footer| {
                        footer.child(
                            div()
                                .text_size(px(10.0))
                                .text_color(rgba(0xfb923cff))
                                .child(self.i18n.t("terminal.broadcast.target_count")),
                        )
                    }),
            );
        }

        menu.into_any_element()
    }

    fn render_terminal_broadcast_menu_action(
        &self,
        item: gpui::Div,
        disabled: bool,
        loading: bool,
        hover_bg: Option<gpui::Rgba>,
        listener: impl Fn(&MouseDownEvent, &mut Window, &mut gpui::App) + 'static,
        cx: &mut Context<Self>,
    ) -> gpui::Div {
        // Tauri broadcast target rows are Radix menu items with a disabled
        // current-terminal row. Keep native hover/cursor and action blocking
        // coupled to the shared context-menu guard.
        self.workspace_context_menu_persistent_styled_action(
            item,
            disabled,
            loading,
            ContextMenuActionableStyle {
                hover_background: hover_bg,
                hover_text_color: None,
            },
            move |_this, event, window, cx| listener(event, window, cx),
            cx,
        )
    }
}

fn terminal_broadcast_menu_left_for_trigger_right(trigger_right: f32) -> f32 {
    (trigger_right - TERMINAL_BROADCAST_MENU_WIDTH).max(12.0)
}

#[cfg(test)]
mod terminal_broadcast_menu_tests {
    use super::*;

    #[test]
    fn broadcast_menu_aligns_end_to_trigger_not_workspace_root() {
        assert_eq!(terminal_broadcast_menu_left_for_trigger_right(700.0), 440.0);
    }

    #[test]
    fn broadcast_menu_keeps_left_viewport_margin_when_trigger_is_narrow() {
        assert_eq!(terminal_broadcast_menu_left_for_trigger_right(120.0), 12.0);
    }
}

#[cfg(test)]
mod privilege_prompt_helper_tests {
    use super::*;
    use chrono::Utc;

    fn saved_privilege_credential(
        id: &str,
        kind: PrivilegeCredentialKind,
        username_hint: Option<&str>,
    ) -> SavedPrivilegeCredential {
        let now = Utc::now();
        SavedPrivilegeCredential {
            id: id.to_string(),
            connection_id: "conn-1".to_string(),
            label: id.to_string(),
            kind,
            username_hint: username_hint.map(str::to_string),
            prompt_patterns: Vec::new(),
            keychain_id: Some(format!("privilege:v1:conn-1:{id}")),
            enabled: true,
            require_click_to_send: true,
            created_at: now,
            updated_at: now,
        }
    }

    #[test]
    fn local_terminal_prompt_helper_is_enabled() {
        assert!(tab_kind_allows_privilege_prompt_helper(
            &TabKind::LocalTerminal
        ));
    }

    #[test]
    fn ssh_terminal_prompt_helper_is_enabled() {
        assert!(tab_kind_allows_privilege_prompt_helper(
            &TabKind::SshTerminal
        ));
    }

    #[test]
    fn prompt_state_survives_without_loaded_credentials() {
        let state = build_privilege_prompt_helper_state(
            "conn-1".to_string(),
            &[],
            "sudo yazi\n[sudo] lipsc 的密码:",
        )
        .expect("localized sudo prompt should create a management state");

        assert_eq!(
            state,
            PrivilegePromptHelperState {
                connection_id: "conn-1".to_string(),
                matches: Vec::new(),
            }
        );
    }

    #[test]
    fn prompt_state_matches_enabled_username_hint() {
        let credentials = vec![
            saved_privilege_credential(
                "other-sudo",
                PrivilegeCredentialKind::SudoPassword,
                Some("other"),
            ),
            saved_privilege_credential(
                "matching-sudo",
                PrivilegeCredentialKind::SudoPassword,
                Some("lipsc"),
            ),
        ];
        let state = build_privilege_prompt_helper_state(
            "conn-1".to_string(),
            &credentials,
            "sudo yazi\n[sudo] lipsc 的密码:",
        )
        .expect("localized sudo prompt should create fill matches");

        assert_eq!(
            state.matches,
            vec![MatchedPrivilegeCredential {
                connection_id: "conn-1".to_string(),
                credential_id: "matching-sudo".to_string(),
                label: "matching-sudo".to_string(),
            }]
        );
    }

    #[test]
    fn generic_sudo_prompt_matches_username_hinted_credential() {
        let credentials = vec![saved_privilege_credential(
            "local-sudo",
            PrivilegeCredentialKind::SudoPassword,
            Some("dominical"),
        )];
        let state =
            build_privilege_prompt_helper_state("local-shell:default".to_string(), &credentials, "❯ sudo yazi\nPassword:")
                .expect("macOS sudo prompt should create fill matches");

        assert_eq!(
            state.matches,
            vec![MatchedPrivilegeCredential {
                connection_id: "local-shell:default".to_string(),
                credential_id: "local-sudo".to_string(),
                label: "local-sudo".to_string(),
            }]
        );
    }
}
