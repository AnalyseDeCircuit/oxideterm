use super::*;
use gpui::StatefulInteractiveElement;
use oxideterm_gpui_ui::{
    ConfirmDialogVariant, ConfirmDialogView, MaterialRole, confirm_dialog_with_focus,
    material_surface, modal::modal_backdrop,
};

use super::rows::{MANAGER_ROW_ACTION_BUTTON, MANAGER_ROW_ACTION_ICON_SIZE};

const SESSION_MANAGER_LIGHT_DIALOG_BACKDROP_ALPHA: u32 = 0x66;
const MANAGER_GROUP_MANAGER_INDENT: f32 = 20.0;

impl CatalogView {
    pub(super) fn session_manager_basic_footer_action(
        &self,
        label: String,
        variant: ButtonVariant,
        action: SessionManagerBasicDialogFooterAction,
        disabled: bool,
        listener: impl Fn(&mut Self, &MouseDownEvent, &mut Window, &mut Context<Self>) + 'static,
        cx: &mut Context<Self>,
    ) -> Div {
        self.session_manager_dialog_footer_action(
            label,
            variant,
            action,
            disabled,
            ButtonSize::Sm,
            None,
            listener,
            cx,
        )
    }

    pub(super) fn session_manager_dialog_footer_action(
        &self,
        label: String,
        variant: ButtonVariant,
        action: SessionManagerBasicDialogFooterAction,
        disabled: bool,
        size: ButtonSize,
        icon: Option<AnyElement>,
        listener: impl Fn(&mut Self, &MouseDownEvent, &mut Window, &mut Context<Self>) + 'static,
        cx: &mut Context<Self>,
    ) -> Div {
        // Mouse activation uses the same disabled/focus-visible ownership as
        // the keyboard FocusCycle path. Keep it centralized so import, group,
        // and auto-route dialogs do not each compose DialogFooter buttons.
        self.workspace_toolbar_action_button(
            label,
            icon,
            ToolbarButtonOptions {
                button: ButtonOptions {
                    variant,
                    size,
                    radius: ButtonRadius::Md,
                    disabled,
                },
                focus_visible: self
                    .session_manager
                    .read(cx)
                    .focused_basic_dialog_footer_action
                    == Some(action),
                ..ToolbarButtonOptions::default()
            },
            cx.listener(listener),
        )
    }

    pub(super) fn render_group_manager_dialog(
        &self,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let (roots, children) = self.session_group_tree();
        let mut group_set = HashSet::new();
        collect_session_group_paths(&roots, &children, &mut group_set);
        let mut groups = group_set.into_iter().collect::<Vec<_>>();
        groups.sort_by_key(|group| group.to_lowercase());
        let has_groups = !groups.is_empty();
        let (editor, group_name, editor_error, manager_error) = {
            let session_manager = self.session_manager.read(cx);
            (
                session_manager.group_editor.clone(),
                session_manager.group_name_draft.clone(),
                session_manager.group_editor_error.clone(),
                session_manager.group_manager_error.clone(),
            )
        };
        let is_rename = matches!(editor, Some(SessionManagerGroupEditor::Rename { .. }));
        let is_subgroup = matches!(
            editor,
            Some(SessionManagerGroupEditor::Create {
                parent_path: Some(_)
            })
        );
        let parent_path = match editor.as_ref() {
            Some(SessionManagerGroupEditor::Create { parent_path })
            | Some(SessionManagerGroupEditor::Rename { parent_path, .. }) => parent_path.clone(),
            None => None,
        };
        let candidate_path = session_group_path_from_leaf(parent_path.as_deref(), &group_name);
        let invalid_leaf_name = !group_name.trim().is_empty() && candidate_path.is_none();
        let editor_has_error = editor_error.is_some() || invalid_leaf_name;
        let editor_message = editor_error.unwrap_or_else(|| {
            self.i18n.t(if invalid_leaf_name {
                "sessionManager.folder_tree.group_name_invalid"
            } else if is_rename {
                "sessionManager.folder_tree.rename_group_description"
            } else if is_subgroup {
                "sessionManager.folder_tree.new_subgroup_description"
            } else {
                "sessionManager.folder_tree.new_group_description"
            })
        });
        let editor_action_key = if is_rename {
            "sessionManager.folder_tree.rename_group"
        } else if is_subgroup {
            "sessionManager.folder_tree.new_subgroup"
        } else {
            "sessionManager.folder_tree.new_group"
        };
        let can_save_group = self.session_manager.read(cx).can_save_group();
        let group_actions_disabled = editor.is_some();
        let workspace = cx.entity();

        let group_rows = groups
            .into_iter()
            .map(|group| {
                let group_depth = group.split('/').count().saturating_sub(1);
                let group_name = group.rsplit('/').next().unwrap_or(&group).to_string();
                let context_group = group.clone();
                let create_subgroup = group.clone();
                let rename_group = group.clone();
                let delete_group = group.clone();
                let create_tooltip = format!(
                    "{} — {group}",
                    self.i18n.t("sessionManager.folder_tree.new_subgroup")
                );
                let rename_tooltip = format!(
                    "{} — {group}",
                    self.i18n.t("sessionManager.folder_tree.rename_group")
                );
                let delete_tooltip = format!(
                    "{} — {group}",
                    self.i18n.t("sessionManager.folder_tree.delete_group")
                );
                let create_workspace = workspace.clone();
                let rename_workspace = workspace.clone();
                let delete_workspace = workspace.clone();
                div()
                    .w_full()
                    .min_w(px(0.0))
                    .h(px(44.0))
                    .pr_2()
                    .pl(px(
                        group_depth as f32 * MANAGER_GROUP_MANAGER_INDENT + self.tokens.spacing.two
                    ))
                    .flex()
                    .items_center()
                    .gap(px(self.tokens.spacing.two))
                    .border_b_1()
                    .border_color(rgb(theme.border))
                    .hover(move |row| row.bg(rgb(theme.bg_hover)))
                    .when(!group_actions_disabled, |row| {
                        row.on_mouse_down(
                            MouseButton::Right,
                            cx.listener(move |this, event: &MouseDownEvent, _window, cx| {
                                this.open_session_manager_context_menu(
                                    SessionManagerRowActionTarget::Group(context_group.clone()),
                                    f32::from(event.position.x),
                                    f32::from(event.position.y),
                                    cx,
                                );
                                cx.stop_propagation();
                            }),
                        )
                    })
                    .child(Self::render_lucide_icon(
                        LucideIcon::Folder,
                        15.0,
                        rgb(theme.warning),
                    ))
                    .child(
                        div()
                            .min_w(px(0.0))
                            .flex_1()
                            .truncate()
                            .text_size(px(MANAGER_ROW_TEXT_SIZE))
                            .child(group_name),
                    )
                    .child(self.workspace_tooltip_icon_button(
                        LucideIcon::FolderPlus,
                        MANAGER_ROW_ACTION_ICON_SIZE,
                        rgb(theme.text),
                        IconButtonOptions {
                            disabled: group_actions_disabled,
                            ..IconButtonOptions::opaque_toolbar(
                                MANAGER_ROW_ACTION_BUTTON,
                                ButtonRadius::Sm,
                            )
                        },
                        create_tooltip,
                        "session-group-manager-create-subgroup",
                        true,
                        cx.listener(move |this, _event, _window, cx| {
                            this.action(
                                CatalogAction::NewSubgroup(create_subgroup.clone()),
                                None,
                                cx,
                            );
                            cx.stop_propagation();
                        }),
                        create_workspace,
                    ))
                    .child(self.workspace_tooltip_icon_button(
                        LucideIcon::Pencil,
                        MANAGER_ROW_ACTION_ICON_SIZE,
                        rgb(theme.text),
                        IconButtonOptions {
                            disabled: group_actions_disabled,
                            ..IconButtonOptions::opaque_toolbar(
                                MANAGER_ROW_ACTION_BUTTON,
                                ButtonRadius::Sm,
                            )
                        },
                        rename_tooltip,
                        "session-group-manager-rename",
                        true,
                        cx.listener(move |this, _event, _window, cx| {
                            this.action(CatalogAction::RenameGroup(rename_group.clone()), None, cx);
                            cx.stop_propagation();
                        }),
                        rename_workspace,
                    ))
                    .child(self.workspace_tooltip_icon_button(
                        LucideIcon::Trash2,
                        MANAGER_ROW_ACTION_ICON_SIZE,
                        rgb(theme.error),
                        IconButtonOptions {
                            disabled: group_actions_disabled,
                            ..IconButtonOptions::opaque_toolbar(
                                MANAGER_ROW_ACTION_BUTTON,
                                ButtonRadius::Sm,
                            )
                        },
                        delete_tooltip,
                        "session-group-manager-delete",
                        true,
                        cx.listener(move |this, _event, _window, cx| {
                            this.action(
                                CatalogAction::Delete(SessionManagerRowActionTarget::Group(
                                    delete_group.clone(),
                                )),
                                None,
                                cx,
                            );
                            cx.stop_propagation();
                        }),
                        delete_workspace,
                    ))
            })
            .collect::<Vec<_>>();

        let row_action_menu = self.session_manager.read(cx).row_action_menu.clone();
        let has_background = self.environment.has_background;

        let dialog = modal_backdrop(rgba(
            SESSION_MANAGER_LIGHT_DIALOG_BACKDROP_ALPHA,
        ))
        .on_mouse_down(
            MouseButton::Left,
            cx.listener(|this, _event, _window, cx| {
                this.action(CatalogAction::CloseGroupManager, None, cx);
                cx.stop_propagation();
            }),
        )
        .child(overlay_content_boundary(
            material_surface(&self.tokens, div(), MaterialRole::Popover)
                .w(px(460.0))
                .flex()
                .flex_col()
                .gap(px(14.0))
                .p(px(16.0))
                .rounded(px(self.tokens.radii.lg))
                .border_1()
                .border_color(rgb(theme.border))
                .child(
                    div()
                        .flex()
                        .items_start()
                        .justify_between()
                        .gap(px(16.0))
                        .child(
                            div()
                                .min_w(px(0.0))
                                .flex_1()
                                .flex()
                                .flex_col()
                                .gap(px(4.0))
                                .child(
                                    div()
                                        .text_size(px(18.0))
                                        .font_weight(gpui::FontWeight::SEMIBOLD)
                                        .child(
                                            self.i18n.t("sessionManager.folder_tree.manage_groups"),
                                        ),
                                )
                                .child(
                                    div()
                                        .text_size(px(self.tokens.metrics.ui_text_sm))
                                        .text_color(rgb(theme.text_muted))
                                        .child(self.i18n.t(
                                            "sessionManager.folder_tree.manage_groups_description",
                                        )),
                                ),
                        )
                        .child(
                            self.workspace_toolbar_action_button(
                                self.i18n.t("sessionManager.folder_tree.new_group"),
                                Some(
                                    Self::render_lucide_icon(
                                        LucideIcon::Plus,
                                        14.0,
                                        rgb(theme.text),
                                    )
                                    .into_any_element(),
                                ),
                                ToolbarButtonOptions {
                                    button: ButtonOptions {
                                        variant: ButtonVariant::Secondary,
                                        size: ButtonSize::Sm,
                                        radius: ButtonRadius::Md,
                                        disabled: editor.is_some(),
                                    },
                                    ..ToolbarButtonOptions::default()
                                },
                                cx.listener(|this, _event, _window, cx| {
                                    this.action(CatalogAction::NewGroup, None, cx);
                                }),
                            ),
                        ),
                )
                .when_some(editor, |dialog, _editor| {
                    dialog.child(
                        div()
                            .flex()
                            .flex_col()
                            .gap(px(10.0))
                            .p_3()
                            .rounded(px(self.tokens.radii.md))
                            .bg(rgb(theme.bg_secondary))
                            .child(
                                div()
                                    .text_size(px(self.tokens.metrics.ui_text_sm))
                                    .font_weight(gpui::FontWeight::SEMIBOLD)
                                    .child(self.i18n.t(editor_action_key)),
                            )
                            .when_some(parent_path.clone(), |editor, parent_path| {
                                editor.child(
                                    div()
                                        .min_w(px(0.0))
                                        .flex()
                                        .items_center()
                                        .gap(px(self.tokens.spacing.two))
                                        .text_size(px(self.tokens.metrics.ui_text_xs))
                                        .text_color(rgb(theme.text_muted))
                                        .child(
                                            self.i18n.t("sessionManager.folder_tree.parent_group"),
                                        )
                                        .child(
                                            div()
                                                .min_w(px(0.0))
                                                .flex_1()
                                                .truncate()
                                                .text_color(rgb(theme.text))
                                                .child(parent_path),
                                        ),
                                )
                            })
                            .child(
                                div()
                                    .text_size(px(self.tokens.metrics.ui_text_xs))
                                    .text_color(rgb(theme.text_muted))
                                    .child(self.i18n.t("sessionManager.folder_tree.group_name")),
                            )
                            .child(
                                self.render_session_text_input(
                                    SessionManagerInput::GroupName,
                                    &group_name,
                                    self.i18n
                                        .t("sessionManager.folder_tree.new_group_placeholder"),
                                    cx,
                                ),
                            )
                            .child(
                                div()
                                    .min_h(px(18.0))
                                    .text_size(px(self.tokens.metrics.ui_text_xs))
                                    .text_color(rgb(if editor_has_error {
                                        theme.error
                                    } else {
                                        theme.text_muted
                                    }))
                                    .child(editor_message),
                            )
                            .child(
                                div()
                                    .flex()
                                    .justify_end()
                                    .gap(px(8.0))
                                    .child(self.session_manager_basic_footer_action(
                                        self.i18n.t("sessionManager.edit_properties.cancel"),
                                        ButtonVariant::Secondary,
                                        SessionManagerBasicDialogFooterAction::Cancel,
                                        false,
                                        |this, _event, _window, cx| {
                                            this.action(CatalogAction::CancelGroupEditor, None, cx);
                                        },
                                        cx,
                                    ))
                                    .child(self.session_manager_basic_footer_action(
                                        self.i18n.t(editor_action_key),
                                        ButtonVariant::Default,
                                        SessionManagerBasicDialogFooterAction::Primary,
                                        !can_save_group,
                                        |this, _event, _window, cx| {
                                            this.action(CatalogAction::SaveGroup, None, cx);
                                        },
                                        cx,
                                    )),
                            ),
                    )
                })
                .when_some(manager_error, |dialog, error| {
                    dialog.child(
                        div()
                            .px_3()
                            .py_2()
                            .rounded(px(self.tokens.radii.md))
                            .bg(rgba((theme.error << 8) | 0x14))
                            .text_size(px(self.tokens.metrics.ui_text_sm))
                            .text_color(rgb(theme.error))
                            .child(error),
                    )
                })
                .child(
                    div()
                        .id("session-group-manager-scroll")
                        .max_h(px(320.0))
                        .overflow_y_scroll().track_scroll(&self.group_scroll)
                        .rounded(px(self.tokens.radii.md))
                        .border_1()
                        .border_color(rgb(theme.border))
                        .when(!group_actions_disabled, |list| {
                            list.on_mouse_down(
                                MouseButton::Right,
                                cx.listener(|this, event: &MouseDownEvent, _window, cx| {
                                    // Empty list space represents the top-level group root.
                                    this.open_session_manager_context_menu(
                                        SessionManagerRowActionTarget::GroupRoot,
                                        f32::from(event.position.x),
                                        f32::from(event.position.y),
                                        cx,
                                    );
                                    cx.stop_propagation();
                                }),
                            )
                        })
                        .children(group_rows)
                        .when(!has_groups, |list| {
                            list.child(
                                div()
                                    .p_3()
                                    .text_size(px(self.tokens.metrics.ui_text_sm))
                                    .text_color(rgb(theme.text_muted))
                                    .child(self.i18n.t("sessionManager.folder_tree.no_groups")),
                            )
                        }),
                )
                .when(!group_actions_disabled, |dialog| {
                    dialog.child(div().flex().justify_end().child(
                        self.session_manager_basic_footer_action(
                            self.i18n.t("sessionManager.folder_tree.close"),
                            ButtonVariant::Secondary,
                            SessionManagerBasicDialogFooterAction::Cancel,
                            false,
                            |this, _event, _window, cx| {
                                this.action(CatalogAction::CloseGroupManager, None, cx);
                            },
                            cx,
                        ),
                    ))
                }),
        ));

        div()
            .size_full()
            .relative()
            .child(dialog)
            .when_some(row_action_menu, |root, menu| {
                let menu =
                    self.render_session_manager_row_action_menu(menu, window, has_background, cx);
                let backdrop = self.context_menu_backdrop(menu, cx).on_scroll(cx.listener(
                    |this, _event, _window, cx| {
                        // Pointer-positioned menus are stale as soon as the group list scrolls.
                        this.close_session_row_menus(cx);
                        cx.stop_propagation();
                    },
                ));
                root.child(backdrop)
            })
            .into_any_element()
    }

    pub(super) fn render_session_manager_delete_confirm(
        &self,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let Some(confirm) = self.session_manager.read(cx).delete_confirm.clone() else {
            return div().into_any_element();
        };
        let (title, confirm_label) = match &confirm {
            SessionManagerDeleteConfirm::Single { name, .. } => (
                confirm_delete_connection_label(&self.i18n, name),
                self.i18n.t("sessionManager.actions.delete"),
            ),
            SessionManagerDeleteConfirm::SerialProfile { name, .. } => (
                self.i18n
                    .t("sessionManager.serial_profiles.confirm_delete")
                    .replace("{{name}}", name),
                self.i18n.t("sessionManager.serial_profiles.delete"),
            ),
            SessionManagerDeleteConfirm::ContainerProfile { name, .. } => (
                self.i18n
                    .t("sessionManager.container_profiles.confirm_delete")
                    .replace("{{name}}", name),
                self.i18n.t("sessionManager.container_profiles.delete"),
            ),
            SessionManagerDeleteConfirm::BleProfile { name, .. } => (
                self.i18n
                    .t("sessionManager.ble_profiles.confirm_delete")
                    .replace("{{name}}", name),
                self.i18n.t("sessionManager.ble_profiles.delete"),
            ),
            SessionManagerDeleteConfirm::LocalTerminalProfile { name, .. } => (
                self.i18n
                    .t("sessionManager.local_terminal_profiles.confirm_delete")
                    .replace("{{name}}", name),
                self.i18n.t("sessionManager.local_terminal_profiles.delete"),
            ),
            SessionManagerDeleteConfirm::TelnetProfile { name, .. } => (
                self.i18n
                    .t("sessionManager.telnet_profiles.confirm_delete")
                    .replace("{{name}}", name),
                self.i18n.t("sessionManager.telnet_profiles.delete"),
            ),
            SessionManagerDeleteConfirm::MoshProfile { name, .. } => (
                self.i18n
                    .t("sessionManager.mosh_profiles.confirm_delete")
                    .replace("{{name}}", name),
                self.i18n.t("sessionManager.mosh_profiles.delete"),
            ),
            SessionManagerDeleteConfirm::StandaloneSftpProfile { name, .. } => (
                self.i18n
                    .t("sessionManager.standalone_sftp_profiles.confirm_delete")
                    .replace("{{name}}", name),
                self.i18n
                    .t("sessionManager.standalone_sftp_profiles.delete"),
            ),
            SessionManagerDeleteConfirm::RemoteDesktopProfile { name, .. } => (
                self.i18n
                    .t("sessionManager.remote_desktop_profiles.confirm_delete")
                    .replace("{{name}}", name),
                self.i18n.t("sessionManager.remote_desktop_profiles.delete"),
            ),
            SessionManagerDeleteConfirm::Batch { targets } => (
                confirm_batch_delete_label(&self.i18n, targets.len()),
                self.i18n.t("common.actions.confirm"),
            ),
            SessionManagerDeleteConfirm::Group { name } => (
                self.i18n
                    .t("sessionManager.folder_tree.confirm_delete_group")
                    .replace("{{name}}", name),
                self.i18n.t("sessionManager.folder_tree.delete_group"),
            ),
        };
        confirm_dialog_with_focus(
            &self.tokens,
            ConfirmDialogView {
                variant: ConfirmDialogVariant::Danger,
                title: div().child(title).into_any_element(),
                description: None,
                cancel_label: div()
                    .child(self.i18n.t("common.actions.cancel"))
                    .into_any_element(),
                confirm_label: div().child(confirm_label).into_any_element(),
            },
            self.session_manager.read(cx).delete_focus,
            cx.listener(|this, _event, _window, cx| {
                this.action(CatalogAction::CancelDelete, None, cx);
                cx.stop_propagation();
            }),
            cx.listener(|this, _event, _window, cx| {
                this.action(CatalogAction::ConfirmDelete, None, cx);
                cx.stop_propagation();
            }),
        )
    }
}
