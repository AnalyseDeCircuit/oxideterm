use crate::{
    SettingsInput, SettingsSelect,
    components::{SettingsCalloutTone, SettingsControls, settings_mono_font_family},
    dialog::settings_dialog_transition,
    entity::{ManagedKeyDialogSnapshot, SettingsWorkspaceEntity},
    select_menu::connection_idle_timeout_label,
    surface::nav::render_icon,
};
use gpui::{
    AnyElement, App, Context, Div, Entity, MouseButton, MouseDownEvent, Render, Rgba, ScrollHandle,
    Subscription, Window, div, prelude::*, px, relative, rgb, rgba,
};
use oxideterm_connections::{
    ConnectionImportDuplicateStrategy, ConnectionImportPreview, ConnectionImportSource,
    ConnectionStore, ImportedConnectionAuthType, ManagedSshKeyInfo, ManagedSshKeyOrigin,
    ManagedSshKeyUsage, SshConfigHost, list_available_ssh_keys, list_ssh_config_hosts,
};
use oxideterm_gpui_ui::{
    ConfirmDialogAction,
    button::{
        ButtonOptions, ButtonRadius, ButtonSize, ButtonVariant, IconButtonOptions,
        ToolbarButtonOptions, icon_button, toolbar_button,
    },
    modal::{
        dialog_content, dialog_description, dialog_footer, dialog_header, dialog_title,
        dismissible_dialog_backdrop,
    },
    scroll::ScrollableElement,
    switch::switch,
    text_input::TextInputContentAlign,
};
use oxideterm_i18n::I18n;
use oxideterm_settings::PersistedSettings;
use std::{collections::HashSet, ops::Deref, rc::Rc};
mod importer;
pub use importer::{
    connection_import_duplicate_strategy_label, connection_import_source_label,
    connection_import_source_options,
};
const CONNECTION_IMPORT_SOURCE_BASIS: f32 = 220.0; // Match the desktop source column while allowing narrow panes to wrap.
const CONNECTION_IMPORT_PATH_BASIS: f32 = 420.0; // Give localized path actions room before they move below the source picker.
const CONNECTION_IMPORT_PREVIEW_ACTIONS_BASIS: f32 = 420.0; // Keep preview controls together until the toolbar wraps.
const CONNECTION_IMPORT_AUTH_WIDTH: f32 = 120.0; // Match the compact trailing authentication column from Tauri.
const SSH_KEY_HEADER_TEXT_BASIS: f32 = 320.0; // Let long localized descriptions wrap before key actions.
const SSH_CONFIG_IMPORT_DIALOG_WIDTH: f32 = 720.0;
const SSH_CONFIG_IMPORT_DIALOG_HEIGHT: f32 = 560.0;

#[derive(Clone)]
pub enum ConnectionRequest {
    LoadHosts(bool),
    SyncHosts(bool),
    ProxyCommand(bool),
    OpenSsh,
    CloseSsh,
    ImportSelectedSsh,
    ImportSsh(String),
    PickImport(bool),
    PreviewImport,
    ApplyImport,
    OpenKeyFile,
    OpenKeyPaste,
    OpenKeyRename(String, String),
    OpenKeyDelete(ManagedSshKeyInfo),
    PickKey,
    ImportKeyFile,
    ImportKeyPaste,
    RenameKey,
    DeleteKey,
    CloseKey,
    ClearFooterFocus,
}

pub struct ConnectionPage<'a> {
    pub controls: SettingsControls<'a>,
    pub state: &'a Entity<SettingsWorkspaceEntity>,
    pub store: &'a ConnectionStore,
    pub dispatch: Rc<dyn Fn(ConnectionRequest, &mut App)>,
    pub scroll: &'a dyn Fn(&str) -> ScrollHandle,
    pub private_key: &'a dyn Fn(&App) -> AnyElement,
    pub footer_focus: Option<ConfirmDialogAction>,
}

impl<'a> Deref for ConnectionPage<'a> {
    type Target = SettingsControls<'a>;

    fn deref(&self) -> &Self::Target {
        &self.controls
    }
}

impl ConnectionPage<'_> {
    fn press(
        &self,
        request: ConnectionRequest,
    ) -> impl Fn(&MouseDownEvent, &mut Window, &mut App) + use<> {
        let dispatch = self.dispatch.clone();
        move |_, _, cx| {
            dispatch(request.clone(), cx);
            cx.stop_propagation();
        }
    }

    fn select<F>(&self, update: F) -> impl Fn(&MouseDownEvent, &mut Window, &mut App) + use<F>
    where
        F: Fn(&mut SettingsWorkspaceEntity, &mut Context<SettingsWorkspaceEntity>) + 'static,
    {
        let state = self.state.downgrade();
        move |_, _, cx| {
            let _ = state.update(cx, |state, cx| update(state, cx));
            cx.stop_propagation();
        }
    }

    fn workspace_toolbar_action_button(
        &self,
        title: String,
        icon: Option<AnyElement>,
        options: ToolbarButtonOptions,
        press: impl Fn(&MouseDownEvent, &mut Window, &mut App) + 'static,
    ) -> Div {
        let enabled = !(options.button.disabled || options.loading);
        toolbar_button(self.tokens, title, icon, options).on_mouse_down(
            MouseButton::Left,
            move |event, window, cx| {
                if enabled {
                    press(event, window, cx);
                } else {
                    cx.stop_propagation();
                }
            },
        )
    }

    fn workspace_icon_action_button(
        &self,
        asset: &'static str,
        size: f32,
        color: Rgba,
        options: IconButtonOptions,
        press: impl Fn(&MouseDownEvent, &mut Window, &mut App) + 'static,
        _: &App,
    ) -> Div {
        let enabled = !(options.disabled || options.loading);
        icon_button(self.tokens, render_icon(asset, size, color), options).on_mouse_down(
            MouseButton::Left,
            move |event, window, cx| {
                if enabled {
                    press(event, window, cx);
                } else {
                    cx.stop_propagation();
                }
            },
        )
    }

    fn standard_footer_action_button(
        &self,
        label: String,
        variant: ButtonVariant,
        action: ConfirmDialogAction,
        disabled: bool,
        press: impl Fn(&MouseDownEvent, &mut Window, &mut App) + 'static,
        _: &App,
    ) -> Div {
        let dispatch = self.dispatch.clone();
        self.workspace_toolbar_action_button(
            label,
            None,
            ToolbarButtonOptions {
                button: ButtonOptions {
                    variant,
                    disabled,
                    size: ButtonSize::Sm,
                    radius: ButtonRadius::Md,
                },
                focus_visible: self.footer_focus == Some(action),
                ..Default::default()
            },
            move |event, window, cx| {
                dispatch(ConnectionRequest::ClearFooterFocus, cx);
                press(event, window, cx);
                cx.stop_propagation();
            },
        )
    }

    pub fn settings_ssh_section(&self, section_index: usize, cx: &App) -> AnyElement {
        if section_index != 0 {
            return div().into_any_element();
        }
        let keys = list_available_ssh_keys();
        let managed_keys = self.store.managed_ssh_keys();
        let mut local_list = div().w_full().min_w_0().flex().flex_col();
        if keys.is_empty() {
            local_list = local_list.child(self.ssh_keys_empty_state());
        } else {
            let key_count = keys.len();
            for (index, key) in keys.into_iter().enumerate() {
                local_list = local_list.child(self.ssh_key_row(key));
                if index + 1 < key_count {
                    local_list = local_list.child(self.card_separator());
                }
            }
        }
        let managed_key_count = managed_keys.len();
        let mut managed_list = div().w_full().min_w_0().flex().flex_col();
        for (index, key) in managed_keys.into_iter().enumerate() {
            managed_list = managed_list.child(self.managed_ssh_key_row(key, cx));
            if index + 1 < managed_key_count {
                managed_list = managed_list.child(self.card_separator());
            }
        }
        let content = div()
            .w_full()
            .min_w_0()
            .flex()
            .flex_col()
            .gap(px(self.tokens.metrics.settings_card_gap))
            .child(self.ssh_key_section_header(
                "settings_view.ssh_keys.local_section",
                "settings_view.ssh_keys.local_description",
                None,
            ))
            .child(local_list)
            .child(self.card_separator())
            .child(self.ssh_key_section_header(
                "settings_view.ssh_keys.managed_section",
                "settings_view.ssh_keys.managed_description",
                Some(self.managed_ssh_key_toolbar(cx)),
            ))
            .when_some(
                self.state.read(cx).managed_key_status().map(str::to_string),
                |section, status| section.child(self.connection_status_row(status)),
            )
            .child(if self.store.managed_ssh_keys().is_empty() {
                self.managed_ssh_keys_empty_state()
            } else {
                managed_list.into_any_element()
            });

        self.settings_card(
            "settings_view.ssh_keys.title",
            "settings_view.ssh_keys.description",
            vec![content.into_any_element()],
        )
    }

    pub fn connection_defaults_section(
        &self,
        settings: &PersistedSettings,
        cx: &App,
    ) -> AnyElement {
        div()
            .w_full()
            .min_w_0()
            .flex()
            .flex_col()
            .gap(px(self.tokens.metrics.settings_card_gap))
            .child(self.connection_labeled_input(
                "settings_view.connections.default_username",
                SettingsInput::ConnectionDefaultUsername,
                settings.connection_defaults.username.clone(),
                settings.connection_defaults.username.clone(),
                cx,
            ))
            .child(self.card_separator())
            .child(self.connection_labeled_input(
                "settings_view.connections.default_port",
                SettingsInput::ConnectionDefaultPort,
                settings.connection_defaults.port.to_string(),
                "22".to_string(),
                cx,
            ))
            .into_any_element()
    }

    fn connection_labeled_input(
        &self,
        label_key: &str,
        input: SettingsInput,
        value: String,
        placeholder: String,
        cx: &App,
    ) -> AnyElement {
        let width = if input == SettingsInput::ConnectionDefaultPort {
            self.tokens.metrics.settings_number_input_width
        } else {
            self.tokens.metrics.settings_select_width
        };
        self.setting_row(
            label_key,
            "",
            self.settings_text_input_control(input, value, placeholder, width, cx),
            cx,
        )
    }

    pub fn connection_idle_timeout_control(
        &self,
        settings: &PersistedSettings,
        cx: &App,
    ) -> AnyElement {
        let select_id = SettingsSelect::ConnectionIdleTimeout;
        let value =
            connection_idle_timeout_label(settings.connection_pool.idle_timeout_secs, &self.i18n);
        let control = self.settings_select_control(
            select_id,
            value,
            false,
            Some(self.tokens.metrics.settings_select_width),
            cx,
        );

        self.setting_row(
            "settings_view.connections.idle_timeout.label",
            "settings_view.connections.idle_timeout.hint",
            control,
            cx,
        )
    }

    pub fn ssh_config_import_section(&self, cx: &App) -> AnyElement {
        let auto_load_hosts = self.settings.ssh_config.auto_load_hosts;
        let auto_sync_hosts = self.settings.ssh_config.auto_sync_hosts;
        let allow_proxy_command = self.settings.ssh_config.allow_proxy_command;
        self.connection_section(
            "settings_view.connections.ssh_config.title",
            "",
            self.separated_settings_rows(vec![
                div()
                    .text_size(px(self.tokens.metrics.ui_text_xs))
                    .text_color(rgb(self.tokens.ui.text_muted))
                    .child(self.ssh_config_source_description())
                    .into_any_element(),
                self.setting_row(
                    "settings_view.connections.ssh_config.auto_load",
                    "settings_view.connections.ssh_config.auto_load_hint",
                    switch(
                        &self.tokens,
                        "ssh-config-auto-load",
                        String::new(),
                        auto_load_hosts,
                    )
                    .on_mouse_down(
                        MouseButton::Left,
                        self.press(ConnectionRequest::LoadHosts(!auto_load_hosts)),
                    )
                    .into_any_element(),
                    cx,
                ),
                self.setting_row(
                    "settings_view.connections.ssh_config.auto_sync",
                    "settings_view.connections.ssh_config.auto_sync_hint",
                    switch(
                        &self.tokens,
                        "ssh-config-auto-sync",
                        String::new(),
                        auto_sync_hosts,
                    )
                    .on_mouse_down(
                        MouseButton::Left,
                        self.press(ConnectionRequest::SyncHosts(!auto_sync_hosts)),
                    )
                    .into_any_element(),
                    cx,
                ),
                self.setting_row(
                    "settings_view.connections.ssh_config.allow_proxy_command",
                    "settings_view.connections.ssh_config.allow_proxy_command_hint",
                    switch(
                        &self.tokens,
                        "ssh-config-allow-proxy-command",
                        String::new(),
                        allow_proxy_command,
                    )
                    .on_mouse_down(
                        MouseButton::Left,
                        self.press(ConnectionRequest::ProxyCommand(!allow_proxy_command)),
                    )
                    .into_any_element(),
                    cx,
                ),
                div()
                    .flex()
                    .justify_start()
                    .child(self.workspace_toolbar_action_button(
                        self.i18n.t("settings_view.connections.ssh_config.open"),
                        Some(render_icon(
                            "lucide/folder-input.svg",
                            16.0,
                            rgb(self.tokens.ui.text),
                        )),
                        self.connection_import_secondary_button_options(false),
                        self.press(ConnectionRequest::OpenSsh),
                    ))
                    .into_any_element(),
            ]),
        )
    }

    pub fn render_settings_ssh_config_import_dialog(&self, cx: &App) -> Option<AnyElement> {
        let dialog = self.state.read(cx).ssh_config_import_snapshot();
        if !dialog.open {
            return None;
        }

        let ssh_hosts = self.settings_ssh_config_hosts();
        let selectable_hosts = ssh_hosts
            .iter()
            .filter(|host| !host.already_imported)
            .map(|host| host.alias.clone())
            .collect::<HashSet<_>>();
        let importable_count = ssh_hosts
            .iter()
            .filter(|host| !host.already_imported)
            .count();
        let selected_count = dialog.selected_hosts.len();
        let all_selected = importable_count > 0 && selected_count == importable_count;
        let backdrop = dismissible_dialog_backdrop()
            .on_mouse_down(MouseButton::Left, self.press(ConnectionRequest::CloseSsh));

        let mut list = div()
            .id("settings-ssh-config-dialog-scroll")
            .w_full()
            .min_w_0()
            .flex_1()
            .min_h(px(0.0))
            .overflow_y_scroll()
            .track_scroll(&(self.scroll)("settings-ssh-config-dialog-scroll"))
            .rounded(px(self.tokens.radii.md))
            .border_1()
            .border_color(rgb(self.tokens.ui.border))
            .bg(self.settings_panel_background(self.tokens.ui.bg_panel))
            .p(px(8.0));
        if ssh_hosts.is_empty() {
            list = list.child(self.ssh_config_empty_state());
        } else {
            for host in ssh_hosts {
                let selected = dialog.selected_hosts.contains(&host.alias);
                list = list.child(self.ssh_config_host_row(host, selected, cx));
            }
        }

        let body = div()
            .flex_1()
            .min_h(px(0.0))
            .px(px(24.0))
            .py(px(18.0))
            .flex()
            .flex_col()
            .gap(px(12.0))
            .when(importable_count > 0, |body| {
                body.child(
                    div()
                        .w_full()
                        .flex()
                        .items_center()
                        .justify_between()
                        .gap(px(12.0))
                        .child(self.ssh_config_toggle_all_button(
                            all_selected,
                            selectable_hosts,
                            cx,
                        ))
                        .when(selected_count > 0, |toolbar| {
                            toolbar.child(self.ssh_config_batch_import_button(selected_count, cx))
                        }),
                )
            })
            .when_some(dialog.status, |body, status| {
                body.child(self.connection_status_row(status))
            })
            .child(list);

        let form = dialog_content(&self.tokens)
            .w(px(SSH_CONFIG_IMPORT_DIALOG_WIDTH))
            .max_w(relative(0.92))
            .h(px(SSH_CONFIG_IMPORT_DIALOG_HEIGHT))
            .max_h(relative(0.88))
            .flex()
            .flex_col()
            .on_mouse_down(MouseButton::Left, |_event, _window, cx| {
                cx.stop_propagation();
            })
            .child(
                dialog_header(&self.tokens)
                    .child(dialog_title(
                        &self.tokens,
                        self.i18n.t("settings_view.connections.ssh_config.title"),
                    ))
                    .child(dialog_description(
                        &self.tokens,
                        self.ssh_config_source_description(),
                    )),
            )
            .child(body)
            .child(
                dialog_footer(&self.tokens).child(self.standard_footer_action_button(
                    self.i18n.t("settings_view.connections.ssh_config.close"),
                    ButtonVariant::Secondary,
                    ConfirmDialogAction::Cancel,
                    false,
                    self.press(ConnectionRequest::CloseSsh),
                    cx,
                )),
            );

        Some(settings_dialog_transition(
            &self.tokens,
            "ssh-config-import-dialog-form",
            backdrop,
            form,
            dialog.presence.phase(),
        ))
    }

    fn settings_ssh_config_hosts(&self) -> Vec<SshConfigHost> {
        let existing_names = self
            .store
            .connections()
            .iter()
            .map(|connection| connection.name.clone())
            .collect::<HashSet<_>>();
        list_ssh_config_hosts(&existing_names).unwrap_or_default()
    }

    pub(super) fn connection_section(
        &self,
        title_key: &str,
        description_key: &str,
        rows: Vec<AnyElement>,
    ) -> AnyElement {
        self.settings_card(title_key, description_key, rows)
    }

    fn ssh_config_toggle_all_button(
        &self,
        all_selected: bool,
        selectable_hosts: HashSet<String>,
        _cx: &App,
    ) -> AnyElement {
        let disabled = selectable_hosts.is_empty();
        let label = if all_selected {
            self.i18n
                .t("settings_view.connections.ssh_config.deselect_all")
        } else {
            self.i18n
                .t("settings_view.connections.ssh_config.select_all")
        };
        div()
            .text_size(px(self.tokens.metrics.ui_text_xs))
            .text_color(rgb(self.tokens.ui.accent_secondary))
            .opacity(if disabled { 0.45 } else { 1.0 })
            .cursor_pointer()
            .hover(|style| style.text_color(rgb(self.tokens.ui.accent_hover)))
            .child(label)
            .when(!disabled, |button| {
                button.on_mouse_down(
                    MouseButton::Left,
                    self.select(move |state, cx| {
                        if all_selected {
                            state.clear_ssh_config_host_selection(cx);
                        } else {
                            state.set_selected_ssh_config_hosts(selectable_hosts.clone(), cx);
                        }
                    }),
                )
            })
            .into_any_element()
    }

    fn ssh_config_batch_import_button(&self, selected_count: usize, _cx: &App) -> AnyElement {
        let label = self
            .i18n
            .t("settings_view.connections.ssh_config.import_selected")
            .replace("{{count}}", &selected_count.to_string());
        // This is the same compact outline action chrome as other migrated
        // settings toolbars; keep it on the workspace action wrapper so click
        // dispatch shares the disabled/loading guard with other Buttons.
        self.workspace_toolbar_action_button(
            label,
            Some(render_icon(
                "lucide/folder-input.svg",
                14.0,
                rgb(self.tokens.ui.text),
            )),
            ToolbarButtonOptions {
                button: ButtonOptions {
                    variant: ButtonVariant::Outline,
                    size: ButtonSize::Sm,
                    radius: ButtonRadius::Md,
                    disabled: false,
                },
                background: Some(self.settings_panel_background(self.tokens.ui.bg_panel)),
                border: Some(rgb(self.tokens.ui.border)),
                text_color: Some(rgb(self.tokens.ui.text)),
                hover_background: Some(rgb(self.tokens.ui.bg_hover)),
                height: Some(28.0),
                padding_x: Some(10.0),
                font_size: Some(self.tokens.metrics.ui_text_xs),
                ..ToolbarButtonOptions::default()
            },
            self.press(ConnectionRequest::ImportSelectedSsh),
        )
        .into_any_element()
    }

    fn ssh_config_host_row(&self, host: SshConfigHost, selected: bool, cx: &App) -> AnyElement {
        let theme = self.tokens.ui;
        let alias = host.alias.clone();
        let disabled = host.already_imported;
        let detail = format!(
            "{}@{}:{}",
            host.user.as_deref().unwrap_or_default(),
            host.hostname.as_deref().unwrap_or(alias.as_str()),
            host.port.unwrap_or(22)
        );

        div()
            .w_full()
            .mb(px(4.0))
            .flex()
            .flex_row()
            .items_center()
            .justify_between()
            .gap(px(12.0))
            .rounded(px(self.tokens.radii.md))
            .border_1()
            .border_color(rgba(0x00000000))
            .bg(rgba(0x00000000))
            .p(px(12.0))
            .opacity(if disabled { 0.5 } else { 1.0 })
            .hover(|style| {
                if disabled {
                    style
                } else {
                    style
                        .bg(rgb(self.tokens.ui.bg_hover))
                        .border_color(rgb(self.tokens.ui.border))
                }
            })
            .child(
                div()
                    .cursor_pointer()
                    .min_w(px(0.0))
                    .flex_1()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap(px(8.0))
                    .child(self.ssh_config_checkbox(selected))
                    .child(
                        div()
                            .min_w(px(0.0))
                            .flex()
                            .flex_col()
                            .gap(px(3.0))
                            .child(
                                div()
                                    .flex()
                                    .flex_row()
                                    .items_center()
                                    .gap(px(8.0))
                                    .child(
                                        div()
                                            .text_size(px(self.tokens.metrics.ui_text_sm))
                                            .font_weight(gpui::FontWeight::MEDIUM)
                                            .text_color(rgb(theme.text))
                                            .child(host.alias.clone()),
                                    )
                                    .when(host.already_imported, |row| {
                                        row.child(self.ssh_config_imported_badge())
                                    }),
                            )
                            .child(
                                div()
                                    .truncate()
                                    .text_size(px(self.tokens.metrics.ui_text_xs))
                                    .text_color(rgb(theme.text_muted))
                                    .child(detail),
                            ),
                    )
                    .when(!disabled, |left| {
                        let alias = alias.clone();
                        left.on_mouse_down(
                            MouseButton::Left,
                            self.select(move |state, cx| {
                                state.toggle_ssh_config_host(alias.clone(), cx)
                            }),
                        )
                    }),
            )
            .child(self.ssh_config_import_button(host.alias, disabled, cx))
            .into_any_element()
    }

    fn ssh_config_import_button(&self, alias: String, disabled: bool, _cx: &App) -> AnyElement {
        // Host-row import uses the shared outline action path but preserves
        // Tauri's pill shape with a post-primitive radius override.
        self.workspace_toolbar_action_button(
            self.i18n.t("settings_view.connections.ssh_config.import"),
            Some(render_icon(
                "lucide/folder-input.svg",
                16.0,
                rgb(self.tokens.ui.text),
            )),
            ToolbarButtonOptions {
                button: ButtonOptions {
                    variant: ButtonVariant::Outline,
                    size: ButtonSize::Sm,
                    radius: ButtonRadius::Md,
                    disabled,
                },
                background: Some(rgba(0x00000000)),
                border: Some(rgb(self.tokens.ui.border)),
                text_color: Some(rgb(self.tokens.ui.text)),
                hover_background: Some(rgb(self.tokens.ui.bg_hover)),
                height: Some(34.0),
                padding_x: Some(14.0),
                font_size: Some(self.tokens.metrics.ui_text_sm),
                ..ToolbarButtonOptions::default()
            },
            self.press(ConnectionRequest::ImportSsh(alias.clone())),
        )
        .rounded_full()
        .into_any_element()
    }

    fn ssh_config_checkbox(&self, checked: bool) -> AnyElement {
        div()
            .size(px(20.0))
            .flex_none()
            .flex()
            .items_center()
            .justify_center()
            .rounded(px(self.tokens.radii.sm))
            .border_1()
            .border_color(rgb(if checked {
                self.tokens.ui.accent
            } else {
                self.tokens.ui.text_muted
            }))
            .bg(if checked {
                rgb(self.tokens.ui.accent)
            } else {
                rgba(0x00000000)
            })
            .text_size(px(12.0))
            .text_color(rgb(self.tokens.ui.accent_text))
            .child(if checked { "✓" } else { "" })
            .into_any_element()
    }

    fn ssh_config_imported_badge(&self) -> AnyElement {
        div()
            .px(px(6.0))
            .py(px(2.0))
            .rounded(px(self.tokens.radii.sm))
            .bg(rgba((self.tokens.ui.accent << 8) | 0x20))
            .text_size(px(self.tokens.metrics.ui_text_2xs))
            .text_color(rgb(self.tokens.ui.accent_secondary))
            .child(
                self.i18n
                    .t("settings_view.connections.ssh_config.already_imported"),
            )
            .into_any_element()
    }

    fn ssh_config_source_description(&self) -> String {
        self.i18n
            .t("settings_view.connections.ssh_config.description")
            .replace(
                "{{path}}",
                &oxideterm_connections::default_ssh_config_path()
                    .display()
                    .to_string(),
            )
    }

    fn ssh_config_empty_state(&self) -> AnyElement {
        div()
            .w_full()
            .max_w(px(672.0))
            .h(px(256.0))
            .rounded(px(self.tokens.radii.md))
            .border_1()
            .border_color(rgb(self.tokens.ui.border))
            .bg(self.settings_panel_background(self.tokens.ui.bg_panel))
            .p(px(8.0))
            .flex()
            .items_center()
            .justify_center()
            .text_align(gpui::TextAlign::Center)
            .text_size(px(self.tokens.metrics.ui_text_sm))
            .text_color(rgb(self.tokens.ui.text_muted))
            .child(self.i18n.t("settings_view.connections.ssh_config.no_hosts"))
            .into_any_element()
    }

    fn ssh_key_row(&self, key: oxideterm_connections::SshKeyInfo) -> AnyElement {
        let theme = self.tokens.ui;
        div()
            .w_full()
            .min_w_0()
            .flex()
            .flex_row()
            .items_center()
            .justify_between()
            .gap(px(16.0))
            .px(px(4.0))
            .py(px(12.0))
            .child(
                div()
                    .min_w(px(0.0))
                    .flex_1()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap(px(12.0))
                    .child(
                        div()
                            .size(px(40.0))
                            .flex_none()
                            .flex()
                            .items_center()
                            .justify_center()
                            .rounded_full()
                            .bg(rgba((theme.accent << 8) | 0x1a))
                            .child(render_icon("lucide/key.svg", 18.0, rgb(theme.accent))),
                    )
                    .child(
                        div()
                            .min_w(px(0.0))
                            .flex_1()
                            .flex()
                            .flex_col()
                            .gap(px(4.0))
                            .child(
                                div()
                                    .truncate()
                                    .text_size(px(self.tokens.metrics.ui_text_sm))
                                    .font_weight(gpui::FontWeight::MEDIUM)
                                    .text_color(rgb(theme.text))
                                    .child(key.name),
                            )
                            .child(
                                div()
                                    .truncate()
                                    .text_size(px(self.tokens.metrics.ui_text_xs))
                                    .text_color(rgb(theme.text_muted))
                                    .child(format!("{} · {}", key.key_type, key.path)),
                            ),
                    ),
            )
            .when(key.has_passphrase, |row| {
                row.child(div().flex_none().child(self.text_badge(
                    self.i18n.t("settings_view.ssh_keys.encrypted"),
                    theme.warning,
                )))
            })
            .into_any_element()
    }

    fn ssh_key_section_header(
        &self,
        title_key: &str,
        description_key: &str,
        actions: Option<AnyElement>,
    ) -> AnyElement {
        div()
            .w_full()
            .min_w_0()
            .flex()
            .flex_row()
            .flex_wrap()
            .items_start()
            .justify_between()
            .gap(px(12.0))
            .child(
                div()
                    .min_w(px(0.0))
                    .flex_1()
                    .flex_basis(px(SSH_KEY_HEADER_TEXT_BASIS))
                    .child(self.card_subsection_header(
                        self.i18n.t(title_key),
                        Some(self.i18n.t(description_key)),
                    )),
            )
            .when_some(actions, |header, actions| header.child(actions))
            .into_any_element()
    }

    fn managed_ssh_key_toolbar(&self, cx: &App) -> AnyElement {
        div()
            .max_w_full()
            .flex()
            .flex_row()
            .flex_wrap()
            .items_center()
            .justify_end()
            .gap(px(8.0))
            .child(self.managed_key_action_button(
                "lucide/file-lock.svg",
                "settings_view.ssh_keys.import_file",
                ButtonVariant::Outline,
                cx,
                self.press(ConnectionRequest::OpenKeyFile),
            ))
            .child(self.managed_key_action_button(
                "lucide/shield-check.svg",
                "settings_view.ssh_keys.paste_key",
                ButtonVariant::Outline,
                cx,
                self.press(ConnectionRequest::OpenKeyPaste),
            ))
            .child(self.managed_key_action_button(
                "lucide/refresh-cw.svg",
                "settings_view.ssh_keys.refresh",
                ButtonVariant::Ghost,
                cx,
                self.select(|settings, cx| settings.clear_managed_key_status(cx)),
            ))
            .into_any_element()
    }

    fn managed_key_action_button(
        &self,
        icon: &'static str,
        label_key: &'static str,
        variant: ButtonVariant,
        _cx: &App,
        handler: impl Fn(&MouseDownEvent, &mut Window, &mut App) + 'static,
    ) -> AnyElement {
        self.workspace_toolbar_action_button(
            self.i18n.t(label_key),
            Some(render_icon(icon, 14.0, rgb(self.tokens.ui.text))),
            ToolbarButtonOptions {
                button: ButtonOptions {
                    variant,
                    size: ButtonSize::Sm,
                    radius: ButtonRadius::Md,
                    disabled: false,
                },
                background: Some(self.settings_panel_background(self.tokens.ui.bg_panel)),
                border: Some(rgb(self.tokens.ui.border)),
                text_color: Some(rgb(self.tokens.ui.text)),
                hover_background: Some(rgb(self.tokens.ui.bg_hover)),
                height: Some(28.0),
                padding_x: Some(10.0),
                font_size: Some(self.tokens.metrics.ui_text_xs),
                ..ToolbarButtonOptions::default()
            },
            handler,
        )
        .into_any_element()
    }

    fn managed_ssh_key_row(&self, key: ManagedSshKeyInfo, cx: &App) -> AnyElement {
        let theme = self.tokens.ui;
        let usage = self
            .store
            .managed_ssh_key_usage(&key.id)
            .map(|usage| usage.count)
            .unwrap_or(0);
        let detail = format!(
            "{} · {} · {}",
            self.managed_key_origin_label(&key.origin),
            if key.requires_passphrase {
                self.i18n.t("settings_view.ssh_keys.passphrase_required")
            } else {
                self.i18n
                    .t("settings_view.ssh_keys.passphrase_not_required")
            },
            self.i18n
                .t("settings_view.ssh_keys.used_by")
                .replace("{{count}}", &usage.to_string())
        );
        div()
            .w_full()
            .min_w_0()
            .flex()
            .flex_row()
            .items_center()
            .justify_between()
            .gap(px(16.0))
            .px(px(4.0))
            .py(px(12.0))
            .child(
                div()
                    .min_w(px(0.0))
                    .flex_1()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap(px(12.0))
                    .child(
                        div()
                            .size(px(40.0))
                            .flex_none()
                            .flex()
                            .items_center()
                            .justify_center()
                            .rounded_full()
                            .bg(rgba((theme.accent << 8) | 0x1a))
                            .child(render_icon(
                                "lucide/shield-check.svg",
                                18.0,
                                rgb(theme.accent),
                            )),
                    )
                    .child(
                        div()
                            .min_w(px(0.0))
                            .flex_1()
                            .flex()
                            .flex_col()
                            .gap(px(4.0))
                            .child(
                                div()
                                    .truncate()
                                    .text_size(px(self.tokens.metrics.ui_text_sm))
                                    .font_weight(gpui::FontWeight::MEDIUM)
                                    .text_color(rgb(theme.text))
                                    .child(key.name.clone()),
                            )
                            .child(
                                div()
                                    .truncate()
                                    .font_family(settings_mono_font_family(self.settings))
                                    .text_size(px(self.tokens.metrics.ui_text_xs))
                                    .text_color(rgb(theme.text_muted))
                                    .child(key.fingerprint.clone()),
                            )
                            .child(
                                div()
                                    .truncate()
                                    .text_size(px(self.tokens.metrics.ui_text_xs))
                                    .text_color(rgb(theme.text_muted))
                                    .child(detail),
                            ),
                    ),
            )
            .child(
                div()
                    .flex_none()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap(px(6.0))
                    .child(self.workspace_icon_action_button(
                        "lucide/pencil.svg",
                        14.0,
                        rgb(theme.text),
                        IconButtonOptions::opaque_toolbar(30.0, ButtonRadius::Md),
                        self.press(ConnectionRequest::OpenKeyRename(
                            key.id.clone(),
                            key.name.clone(),
                        )),
                        cx,
                    ))
                    .child(self.workspace_icon_action_button(
                        "lucide/trash-2.svg",
                        14.0,
                        rgb(theme.error),
                        IconButtonOptions {
                            hover_background: Some(rgba((theme.error << 8) | 0x14)),
                            ..IconButtonOptions::opaque_toolbar(30.0, ButtonRadius::Md)
                        },
                        self.press(ConnectionRequest::OpenKeyDelete(key)),
                        cx,
                    )),
            )
            .into_any_element()
    }

    fn managed_ssh_keys_empty_state(&self) -> AnyElement {
        div()
            .w_full()
            .min_w_0()
            .py(px(24.0))
            .text_align(gpui::TextAlign::Center)
            .text_size(px(self.tokens.metrics.ui_text_sm))
            .text_color(rgb(self.tokens.ui.text_muted))
            .child(self.i18n.t("settings_view.ssh_keys.no_managed_keys"))
            .into_any_element()
    }

    fn managed_key_origin_label(&self, origin: &ManagedSshKeyOrigin) -> String {
        match origin {
            ManagedSshKeyOrigin::ImportedFile => {
                self.i18n.t("settings_view.ssh_keys.origin_imported_file")
            }
            ManagedSshKeyOrigin::PastedText => {
                self.i18n.t("settings_view.ssh_keys.origin_pasted_text")
            }
            ManagedSshKeyOrigin::OxideImport => {
                self.i18n.t("settings_view.ssh_keys.origin_oxide_import")
            }
        }
    }

    pub fn render_settings_managed_key_dialog(&self, cx: &App) -> Option<AnyElement> {
        match self.state.read(cx).managed_key_dialog_snapshot()? {
            ManagedKeyDialogSnapshot::ImportFile {
                file_path,
                file_name,
                presence,
            } => Some(self.render_settings_managed_key_import_file_dialog(
                file_path, file_name, presence, cx,
            )),
            ManagedKeyDialogSnapshot::Paste {
                name,
                private_key_present,
                presence,
            } => Some(self.render_settings_managed_key_paste_dialog(
                name,
                private_key_present,
                presence,
                cx,
            )),
            ManagedKeyDialogSnapshot::Rename { name, presence } => {
                Some(self.render_settings_managed_key_rename_dialog(name, presence, cx))
            }
            ManagedKeyDialogSnapshot::Delete {
                key,
                usage,
                presence,
            } => Some(self.render_settings_managed_key_delete_dialog(key, usage, presence, cx)),
        }
    }

    fn render_settings_managed_key_import_file_dialog(
        &self,
        file_path: String,
        file_name: String,
        presence: oxideterm_gpui_ui::motion::ExitPresence,
        cx: &App,
    ) -> AnyElement {
        let can_import = !file_path.trim().is_empty();
        self.settings_managed_key_dialog_frame(
            "modals.managed_key.import_file.title",
            "modals.managed_key.import_file.description",
            vec![
                self.settings_managed_key_input_field(
                    "modals.managed_key.import_file.path",
                    SettingsInput::ManagedKeyFilePath,
                    file_path,
                    "~/.ssh/id_ed25519".to_string(),
                    420.0,
                    cx,
                ),
                div()
                    .flex()
                    .justify_start()
                    .child(self.managed_key_dialog_button(
                        self.i18n.t("modals.managed_key.import_file.browse_title"),
                        ButtonVariant::Outline,
                        false,
                        self.press(ConnectionRequest::PickKey),
                        cx,
                    ))
                    .into_any_element(),
                self.settings_managed_key_input_field(
                    "modals.managed_key.display_name",
                    SettingsInput::ManagedKeyFileName,
                    file_name,
                    "Managed SSH Key".to_string(),
                    420.0,
                    cx,
                ),
                self.settings_managed_key_secret_input_field(
                    "modals.managed_key.passphrase",
                    SettingsInput::ManagedKeyFilePassphrase,
                    String::new(),
                    self.i18n.t("modals.managed_key.passphrase_placeholder"),
                    420.0,
                    cx,
                ),
                self.settings_managed_key_hint("modals.managed_key.custody_hint"),
            ],
            self.i18n.t("modals.managed_key.import"),
            can_import,
            self.press(ConnectionRequest::ImportKeyFile),
            presence.phase(),
            cx,
        )
    }

    fn render_settings_managed_key_paste_dialog(
        &self,
        name: String,
        private_key_present: bool,
        presence: oxideterm_gpui_ui::motion::ExitPresence,
        cx: &App,
    ) -> AnyElement {
        let can_import = private_key_present;
        self.settings_managed_key_dialog_frame(
            "modals.managed_key.paste.title",
            "modals.managed_key.paste.description",
            vec![
                self.settings_managed_key_input_field(
                    "modals.managed_key.display_name",
                    SettingsInput::ManagedKeyPasteName,
                    name,
                    "Managed SSH Key".to_string(),
                    420.0,
                    cx,
                ),
                (self.private_key)(cx),
                self.settings_managed_key_secret_input_field(
                    "modals.managed_key.passphrase",
                    SettingsInput::ManagedKeyPastePassphrase,
                    String::new(),
                    self.i18n.t("modals.managed_key.passphrase_placeholder"),
                    420.0,
                    cx,
                ),
                self.settings_managed_key_hint("modals.managed_key.custody_hint"),
            ],
            self.i18n.t("modals.managed_key.import"),
            can_import,
            self.press(ConnectionRequest::ImportKeyPaste),
            presence.phase(),
            cx,
        )
    }

    fn render_settings_managed_key_rename_dialog(
        &self,
        name: String,
        presence: oxideterm_gpui_ui::motion::ExitPresence,
        cx: &App,
    ) -> AnyElement {
        let can_save = !name.trim().is_empty();
        self.settings_managed_key_dialog_frame(
            "settings_view.ssh_keys.rename_title",
            "settings_view.ssh_keys.managed_description",
            vec![self.settings_managed_key_input_field(
                "settings_view.ssh_keys.rename_name",
                SettingsInput::ManagedKeyRenameName,
                name,
                "Managed SSH Key".to_string(),
                420.0,
                cx,
            )],
            self.i18n.t("settings_view.ssh_keys.rename"),
            can_save,
            self.press(ConnectionRequest::RenameKey),
            presence.phase(),
            cx,
        )
    }

    fn render_settings_managed_key_delete_dialog(
        &self,
        key: ManagedSshKeyInfo,
        usage: ManagedSshKeyUsage,
        presence: oxideterm_gpui_ui::motion::ExitPresence,
        cx: &App,
    ) -> AnyElement {
        let can_delete = usage.count == 0;
        let mut description = if can_delete {
            self.i18n
                .t("settings_view.ssh_keys.delete_unused_description")
                .replace("{{name}}", &key.name)
        } else {
            self.i18n
                .t("settings_view.ssh_keys.delete_blocked_description")
                .replace("{{count}}", &usage.count.to_string())
        };
        if !usage.items.is_empty() {
            let used_by = usage
                .items
                .iter()
                .map(|item| format!("{} ({})", item.connection_name, item.location))
                .collect::<Vec<_>>()
                .join(", ");
            description.push_str("\n");
            description.push_str(&used_by);
        }
        self.settings_managed_key_dialog_frame(
            "settings_view.ssh_keys.delete_title",
            "",
            vec![
                div()
                    .text_size(px(self.tokens.metrics.ui_text_sm))
                    .text_color(rgb(self.tokens.ui.text_muted))
                    .child(description)
                    .into_any_element(),
            ],
            self.i18n.t("settings_view.ssh_keys.delete"),
            can_delete,
            self.press(ConnectionRequest::DeleteKey),
            presence.phase(),
            cx,
        )
    }

    fn settings_managed_key_dialog_frame(
        &self,
        title_key: &str,
        description_key: &str,
        rows: Vec<AnyElement>,
        confirm_label: String,
        can_confirm: bool,
        confirm: impl Fn(&MouseDownEvent, &mut Window, &mut App) + 'static,
        phase: oxideterm_gpui_ui::motion::ExitPhase,
        cx: &App,
    ) -> AnyElement {
        let backdrop = dismissible_dialog_backdrop()
            .on_mouse_down(MouseButton::Left, self.press(ConnectionRequest::CloseKey));
        let form = dialog_content(&self.tokens)
            .w(px(520.0))
            .max_w(relative(0.92))
            .max_h(relative(0.90))
            .flex()
            .flex_col()
            .on_mouse_down(MouseButton::Left, |_event, _window, cx| {
                cx.stop_propagation();
            })
            .child(
                dialog_header(&self.tokens)
                    .flex_shrink_0()
                    .child(dialog_title(&self.tokens, self.i18n.t(title_key)))
                    .when(!description_key.is_empty(), |header| {
                        header.child(dialog_description(
                            &self.tokens,
                            self.i18n.t(description_key),
                        ))
                    }),
            )
            .child(
                div()
                    .id("managed-key-dialog-scroll")
                    .min_h(px(0.0))
                    .overflow_y_scroll()
                    .track_scroll(&(self.scroll)("managed-key-dialog-scroll"))
                    .vertical_scrollbar(&(self.scroll)("managed-key-dialog-scroll"))
                    .px(px(24.0))
                    .py(px(18.0))
                    .flex()
                    .flex_col()
                    .gap(px(12.0))
                    .children(rows),
            )
            .child(
                dialog_footer(&self.tokens)
                    .flex_shrink_0()
                    .child(self.standard_footer_action_button(
                        self.i18n.t("common.actions.cancel"),
                        ButtonVariant::Outline,
                        ConfirmDialogAction::Cancel,
                        false,
                        self.press(ConnectionRequest::CloseKey),
                        cx,
                    ))
                    .child(
                        self.standard_footer_action_button(
                            confirm_label,
                            ButtonVariant::Default,
                            ConfirmDialogAction::Confirm,
                            !can_confirm,
                            confirm,
                            cx,
                        )
                        .debug_selector(|| "managed-key-confirm".into()),
                    ),
            );
        settings_dialog_transition(
            &self.tokens,
            "managed-key-dialog-form",
            backdrop,
            form,
            phase,
        )
    }

    fn settings_managed_key_input_field(
        &self,
        label_key: &str,
        input: SettingsInput,
        value: impl AsRef<str>,
        placeholder: String,
        width: f32,
        cx: &App,
    ) -> AnyElement {
        div()
            .flex()
            .flex_col()
            .gap(px(8.0))
            .child(
                div()
                    .text_size(px(self.tokens.metrics.ui_text_sm))
                    .font_weight(gpui::FontWeight::MEDIUM)
                    .text_color(rgb(self.tokens.ui.text))
                    .child(self.i18n.t(label_key)),
            )
            .child(self.settings_text_input_control(input, value.as_ref(), placeholder, width, cx))
            .into_any_element()
    }

    fn settings_managed_key_secret_input_field(
        &self,
        label_key: &str,
        input: SettingsInput,
        value: impl AsRef<str>,
        placeholder: String,
        width: f32,
        cx: &App,
    ) -> AnyElement {
        div()
            .flex()
            .flex_col()
            .gap(px(8.0))
            .child(
                div()
                    .text_size(px(self.tokens.metrics.ui_text_sm))
                    .font_weight(gpui::FontWeight::MEDIUM)
                    .text_color(rgb(self.tokens.ui.text))
                    .child(self.i18n.t(label_key)),
            )
            .child((self.field)(
                input,
                value.as_ref(),
                placeholder,
                Some(width),
                TextInputContentAlign::Start,
                true,
                cx,
            ))
            .into_any_element()
    }

    fn settings_managed_key_hint(&self, label_key: &str) -> AnyElement {
        div()
            .text_size(px(self.tokens.metrics.ui_text_xs))
            .text_color(rgb(self.tokens.ui.text_muted))
            .child(self.i18n.t(label_key))
            .into_any_element()
    }

    fn managed_key_dialog_button(
        &self,
        label: String,
        variant: ButtonVariant,
        disabled: bool,
        listener: impl Fn(&MouseDownEvent, &mut Window, &mut App) + 'static,
        _cx: &App,
    ) -> Div {
        self.workspace_toolbar_action_button(
            label,
            None,
            ToolbarButtonOptions {
                button: ButtonOptions {
                    variant,
                    size: ButtonSize::Sm,
                    radius: ButtonRadius::Md,
                    disabled,
                },
                ..ToolbarButtonOptions::default()
            },
            listener,
        )
    }

    fn ssh_keys_empty_state(&self) -> AnyElement {
        div()
            .w_full()
            .py(px(24.0))
            .text_align(gpui::TextAlign::Center)
            .text_size(px(self.tokens.metrics.ui_text_sm))
            .text_color(rgb(self.tokens.ui.text_muted))
            .child(self.i18n.t("settings_view.ssh_keys.no_keys"))
            .into_any_element()
    }

    fn connection_status_row(&self, status: String) -> AnyElement {
        self.settings_callout(
            SettingsCalloutTone::Info,
            None,
            div()
                .text_size(px(self.tokens.metrics.ui_text_xs))
                .child(status)
                .into_any_element(),
        )
    }
}

pub struct ConnectionDialog {
    body: Rc<dyn Fn(&App) -> Option<AnyElement>>,
    _state_changed: Subscription,
}

impl ConnectionDialog {
    pub fn new(
        settings: &Entity<SettingsWorkspaceEntity>,
        managed_key: bool,
        body: Rc<dyn Fn(&App) -> Option<AnyElement>>,
        cx: &mut Context<Self>,
    ) -> Self {
        let state_changed = cx.observe(settings, move |_, state, cx| {
            let state = state.read(cx);
            if if managed_key {
                state.managed_key_dialog_open()
            } else {
                state.ssh_config_import_dialog_open()
            } {
                cx.notify();
            }
        });
        Self {
            body,
            _state_changed: state_changed,
        }
    }
}

impl Render for ConnectionDialog {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        (self.body)(cx).unwrap_or_else(|| div().into_any_element())
    }
}
