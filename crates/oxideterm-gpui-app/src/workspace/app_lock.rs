use super::*;

use oxideterm_app_lock::{
    AppLockStore, BiometricAvailability, BiometricOutcome, authenticate_biometric,
    biometric_availability,
};
use oxideterm_gpui_settings_view::SettingsInput;
use oxideterm_gpui_ui::{
    button::{ButtonOptions, ButtonRadius, ButtonSize, ButtonVariant, ToolbarButtonOptions},
    modal::{
        dialog_content, dialog_description, dialog_footer, dialog_header, dialog_title,
        dismissible_dialog_backdrop,
    },
};
use zeroize::{Zeroize, Zeroizing};

const APP_LOCK_DIALOG_WIDTH: f32 = 460.0;
const APP_LOCK_SCREEN_CARD_WIDTH: f32 = 460.0;
const APP_LOCK_SCREEN_DESCRIPTION_WIDTH: f32 = 390.0;
const APP_LOCK_MINIMUM_PASSWORD_LENGTH: usize = 6;
const APP_LOCK_FAILURE_COOLDOWN: Duration = Duration::from_secs(1);
const APP_LOCK_EXTENDED_COOLDOWN: Duration = Duration::from_secs(30);
const APP_LOCK_EXTENDED_COOLDOWN_THRESHOLD: u32 = 5;

fn app_lock_failure_cooldown(failed_attempts: u32) -> Duration {
    if failed_attempts >= APP_LOCK_EXTENDED_COOLDOWN_THRESHOLD {
        APP_LOCK_EXTENDED_COOLDOWN
    } else {
        APP_LOCK_FAILURE_COOLDOWN
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum AppLockDialog {
    Configure,
    Change,
    Remove,
}

pub(super) struct AppLockState {
    store: AppLockStore,
    pub(super) configured: bool,
    pub(super) locked: bool,
    pub(super) dialog: Option<AppLockDialog>,
    lock_after_configure: bool,
    pending: bool,
    biometric_available: bool,
    biometric_check_pending: bool,
    current_password: String,
    new_password: String,
    confirm_password: String,
    error: Option<String>,
    failed_attempts: u32,
    retry_at: Option<Instant>,
}

impl AppLockState {
    pub(super) fn load(store: AppLockStore) -> Self {
        let (configured, error) = match store.is_configured() {
            Ok(configured) => (configured, None),
            Err(_) => (
                false,
                Some("settings_view.general.app_lock_unavailable".to_string()),
            ),
        };
        Self {
            store,
            configured,
            locked: false,
            dialog: None,
            lock_after_configure: false,
            pending: false,
            biometric_available: false,
            biometric_check_pending: false,
            current_password: String::new(),
            new_password: String::new(),
            confirm_password: String::new(),
            error,
            failed_attempts: 0,
            retry_at: None,
        }
    }

    fn clear_passwords(&mut self) {
        self.current_password.zeroize();
        self.new_password.zeroize();
        self.confirm_password.zeroize();
        self.current_password.clear();
        self.new_password.clear();
        self.confirm_password.clear();
    }

    fn retry_blocked(&self) -> bool {
        self.retry_at
            .is_some_and(|retry_at| Instant::now() < retry_at)
    }
}

impl Drop for AppLockState {
    fn drop(&mut self) {
        self.clear_passwords();
    }
}

impl WorkspaceApp {
    pub(in crate::workspace) fn activate_app_lock(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.app_lock.configured {
            self.open_app_lock_dialog(AppLockDialog::Configure, true, window, cx);
            return;
        }

        // Locking releases forwarded input before sensitive surfaces stop rendering,
        // preventing a held key or mouse button from remaining active remotely.
        self.release_active_remote_desktop_inputs();
        self.finish_sidebar_resize(cx);
        self.finish_ai_sidebar_resize(cx);
        self.finish_split_drag(cx);
        self.close_terminal_command_overlays(cx);
        self.clear_workspace_tooltip("activity-app-lock", cx);
        self.clear_app_lock_input_state();
        self.app_lock.locked = true;
        self.app_lock.error = None;
        self.refresh_app_lock_biometric_availability(cx);
        self.focus_app_lock_input(SettingsInput::AppLockCurrentPassword, window, cx);
        window.set_window_title(&SharedString::from(
            self.i18n.t("settings_view.general.app_lock_window_title"),
        ));
        // Detached tab windows share the workspace lock state but own separate render roots.
        cx.refresh_windows();
    }

    pub(in crate::workspace) fn open_app_lock_dialog(
        &mut self,
        dialog: AppLockDialog,
        lock_after_configure: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.clear_app_lock_input_state();
        self.app_lock.dialog = Some(dialog);
        self.app_lock.lock_after_configure = lock_after_configure;
        self.app_lock.error = None;
        let first_input = match dialog {
            AppLockDialog::Configure => SettingsInput::AppLockNewPassword,
            AppLockDialog::Change | AppLockDialog::Remove => SettingsInput::AppLockCurrentPassword,
        };
        self.focus_app_lock_input(first_input, window, cx);
        cx.notify();
    }

    pub(in crate::workspace) fn close_app_lock_dialog(&mut self, cx: &mut Context<Self>) {
        if self.app_lock.pending {
            return;
        }
        self.app_lock.dialog = None;
        self.app_lock.lock_after_configure = false;
        self.clear_app_lock_input_state();
        cx.notify();
    }

    fn focus_app_lock_input(
        &mut self,
        input: SettingsInput,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let current = self.app_lock_input_value(input).to_string();
        self.focus_settings_input(input, current, cx);
        window.focus(&self.focus_handle, cx);
    }

    pub(in crate::workspace) fn app_lock_input_value(&self, input: SettingsInput) -> &str {
        match input {
            SettingsInput::AppLockCurrentPassword => &self.app_lock.current_password,
            SettingsInput::AppLockNewPassword => &self.app_lock.new_password,
            SettingsInput::AppLockConfirmPassword => &self.app_lock.confirm_password,
            _ => "",
        }
    }

    pub(in crate::workspace) fn set_app_lock_input_value(
        &mut self,
        input: SettingsInput,
        value: &str,
    ) -> bool {
        let target = match input {
            SettingsInput::AppLockCurrentPassword => &mut self.app_lock.current_password,
            SettingsInput::AppLockNewPassword => &mut self.app_lock.new_password,
            SettingsInput::AppLockConfirmPassword => &mut self.app_lock.confirm_password,
            _ => return false,
        };
        target.zeroize();
        target.push_str(value);
        self.app_lock.error = None;
        true
    }

    fn commit_focused_app_lock_input(&mut self) {
        let Some(input) = self.focused_settings_input.filter(|input| {
            matches!(
                input,
                SettingsInput::AppLockCurrentPassword
                    | SettingsInput::AppLockNewPassword
                    | SettingsInput::AppLockConfirmPassword
            )
        }) else {
            return;
        };
        let mut draft = std::mem::take(&mut self.settings_input_draft);
        let _ = self.set_app_lock_input_value(input, &draft);
        draft.zeroize();
        self.focused_settings_input = None;
        self.clear_ime_selection();
    }

    fn clear_app_lock_input_state(&mut self) {
        if self.focused_settings_input.is_some_and(|input| {
            matches!(
                input,
                SettingsInput::AppLockCurrentPassword
                    | SettingsInput::AppLockNewPassword
                    | SettingsInput::AppLockConfirmPassword
            )
        }) {
            self.focused_settings_input = None;
        }
        self.settings_input_draft.zeroize();
        self.settings_input_draft.clear();
        self.app_lock.clear_passwords();
        self.ime_marked_text = None;
        self.clear_ime_selection();
    }

    pub(in crate::workspace) fn submit_app_lock_dialog(&mut self, cx: &mut Context<Self>) {
        if self.app_lock.pending {
            return;
        }
        self.commit_focused_app_lock_input();
        let Some(dialog) = self.app_lock.dialog else {
            return;
        };

        if matches!(dialog, AppLockDialog::Configure | AppLockDialog::Change) {
            if self.app_lock.new_password.chars().count() < APP_LOCK_MINIMUM_PASSWORD_LENGTH {
                self.app_lock.error = Some(
                    self.i18n
                        .t("settings_view.general.app_lock_password_too_short"),
                );
                cx.notify();
                return;
            }
            if self.app_lock.new_password != self.app_lock.confirm_password {
                self.app_lock.error = Some(
                    self.i18n
                        .t("settings_view.general.app_lock_password_mismatch"),
                );
                cx.notify();
                return;
            }
        }
        if matches!(dialog, AppLockDialog::Change | AppLockDialog::Remove)
            && self.app_lock.current_password.is_empty()
        {
            self.app_lock.error = Some(
                self.i18n
                    .t("settings_view.general.app_lock_password_required"),
            );
            cx.notify();
            return;
        }

        let current_password = Zeroizing::new(std::mem::take(&mut self.app_lock.current_password));
        let new_password = Zeroizing::new(std::mem::take(&mut self.app_lock.new_password));
        self.app_lock.confirm_password.zeroize();
        self.app_lock.confirm_password.clear();
        self.app_lock.pending = true;
        self.app_lock.error = None;
        let store = self.app_lock.store.clone();
        let runtime = self.forwarding_runtime.handle().clone();
        cx.spawn(async move |weak, cx| {
            let task = runtime.spawn_blocking(move || match dialog {
                AppLockDialog::Configure => store.set_password(new_password).map(|_| true),
                AppLockDialog::Change => store.change_password(current_password, new_password),
                AppLockDialog::Remove => store.remove_password(current_password),
            });
            let result = task
                .await
                .map_err(|error| error.to_string())
                .and_then(|result| result.map_err(|error| error.to_string()));
            let _ = weak.update(cx, |this, cx| {
                this.app_lock.pending = false;
                match result {
                    Ok(true) => {
                        let should_lock = dialog == AppLockDialog::Configure
                            && this.app_lock.lock_after_configure;
                        this.app_lock.configured = dialog != AppLockDialog::Remove;
                        this.app_lock.dialog = None;
                        this.app_lock.lock_after_configure = false;
                        this.clear_app_lock_input_state();
                        if should_lock {
                            this.app_lock.locked = true;
                            this.focused_settings_input =
                                Some(SettingsInput::AppLockCurrentPassword);
                            this.refresh_app_lock_biometric_availability(cx);
                        }
                        cx.refresh_windows();
                    }
                    Ok(false) => {
                        this.app_lock.error = Some(
                            this.i18n
                                .t("settings_view.general.app_lock_incorrect_password"),
                        );
                    }
                    Err(_) => {
                        this.app_lock.error = Some(
                            this.i18n
                                .t("settings_view.general.app_lock_operation_failed"),
                        );
                        this.focused_settings_input = Some(SettingsInput::AppLockCurrentPassword);
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    pub(in crate::workspace) fn submit_app_unlock(&mut self, cx: &mut Context<Self>) {
        if self.app_lock.pending || self.app_lock.retry_blocked() {
            return;
        }
        self.commit_focused_app_lock_input();
        if self.app_lock.current_password.is_empty() {
            self.app_lock.error = Some(
                self.i18n
                    .t("settings_view.general.app_lock_password_required"),
            );
            cx.notify();
            return;
        }

        let password = Zeroizing::new(std::mem::take(&mut self.app_lock.current_password));
        let store = self.app_lock.store.clone();
        let runtime = self.forwarding_runtime.handle().clone();
        self.app_lock.pending = true;
        self.app_lock.error = None;
        cx.spawn(async move |weak, cx| {
            let task = runtime.spawn_blocking(move || store.verify_password(password));
            let result = task
                .await
                .map_err(|error| error.to_string())
                .and_then(|result| result.map_err(|error| error.to_string()));
            let _ = weak.update(cx, |this, cx| {
                this.app_lock.pending = false;
                match result {
                    Ok(true) => {
                        this.app_lock.locked = false;
                        this.app_lock.failed_attempts = 0;
                        this.app_lock.retry_at = None;
                        this.clear_app_lock_input_state();
                        this.needs_active_pane_focus = true;
                        cx.refresh_windows();
                    }
                    Ok(false) => {
                        this.register_app_lock_failure(cx);
                    }
                    Err(_) => {
                        this.app_lock.error = Some(
                            this.i18n
                                .t("settings_view.general.app_lock_operation_failed"),
                        );
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    pub(in crate::workspace) fn submit_biometric_unlock(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.app_lock.pending || !self.app_lock.biometric_available {
            return;
        }
        let reason = self
            .i18n
            .t("settings_view.general.app_lock_biometric_reason");
        let native_window_handle = app_lock_native_window_handle(window);
        let runtime = self.forwarding_runtime.handle().clone();
        self.app_lock.pending = true;
        self.app_lock.error = None;
        cx.spawn(async move |weak, cx| {
            let task = runtime
                .spawn_blocking(move || authenticate_biometric(&reason, native_window_handle));
            let result = task
                .await
                .map_err(|error| error.to_string())
                .and_then(|result| result.map_err(|error| error.to_string()));
            let _ = weak.update(cx, |this, cx| {
                this.app_lock.pending = false;
                match result {
                    Ok(BiometricOutcome::Verified) => {
                        this.app_lock.locked = false;
                        this.app_lock.failed_attempts = 0;
                        this.app_lock.retry_at = None;
                        this.clear_app_lock_input_state();
                        this.needs_active_pane_focus = true;
                        cx.refresh_windows();
                    }
                    Ok(BiometricOutcome::Canceled) => {
                        // Cancellation is not an authentication failure; password remains ready.
                    }
                    Ok(BiometricOutcome::Failed) => {
                        this.app_lock.error =
                            Some("settings_view.general.app_lock_biometric_failed".to_string());
                    }
                    Ok(BiometricOutcome::Unavailable) => {
                        this.app_lock.biometric_available = false;
                        this.app_lock.error = Some(
                            "settings_view.general.app_lock_biometric_unavailable".to_string(),
                        );
                    }
                    Err(_) => {
                        this.app_lock.error =
                            Some("settings_view.general.app_lock_biometric_failed".to_string());
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    fn refresh_app_lock_biometric_availability(&mut self, cx: &mut Context<Self>) {
        if self.app_lock.biometric_check_pending {
            return;
        }
        self.app_lock.biometric_available = false;
        self.app_lock.biometric_check_pending = true;
        let runtime = self.forwarding_runtime.handle().clone();
        cx.spawn(async move |weak, cx| {
            // Windows availability is a WinRT async operation; keep it off the GPUI thread.
            let availability = runtime.spawn_blocking(biometric_availability).await;
            let _ = weak.update(cx, |this, cx| {
                this.app_lock.biometric_check_pending = false;
                this.app_lock.biometric_available =
                    matches!(availability, Ok(BiometricAvailability::Available));
                cx.notify();
            });
        })
        .detach();
    }

    fn register_app_lock_failure(&mut self, cx: &mut Context<Self>) {
        self.app_lock.failed_attempts = self.app_lock.failed_attempts.saturating_add(1);
        let cooldown = app_lock_failure_cooldown(self.app_lock.failed_attempts);
        self.app_lock.retry_at = Some(Instant::now() + cooldown);
        self.app_lock.error = Some(self.i18n.t(if cooldown == APP_LOCK_EXTENDED_COOLDOWN {
            "settings_view.general.app_lock_too_many_attempts"
        } else {
            "settings_view.general.app_lock_incorrect_password"
        }));
        self.focused_settings_input = Some(SettingsInput::AppLockCurrentPassword);
        cx.spawn(async move |weak, cx| {
            Timer::after(cooldown).await;
            let _ = weak.update(cx, |this, cx| {
                if this
                    .app_lock
                    .retry_at
                    .is_some_and(|retry_at| Instant::now() >= retry_at)
                {
                    this.app_lock.retry_at = None;
                    cx.notify();
                }
            });
        })
        .detach();
    }

    pub(in crate::workspace) fn handle_app_lock_key(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.defer_active_ime_key(&event.keystroke, window, cx) {
            return;
        }
        let key = event.keystroke.key.as_str();
        if matches!(key, "enter" | "return") {
            self.submit_app_unlock(cx);
        } else if key == "escape" {
        } else if self.handle_active_text_input_edit_shortcut(&event.keystroke, cx)
            || self.handle_active_text_input_delete_selection(&event.keystroke, cx)
            || self.handle_active_text_input_navigation(&event.keystroke, cx)
        {
        } else {
            let _ = self.handle_settings_input_key(event, cx);
        }
        window.prevent_default();
        cx.stop_propagation();
    }

    pub(in crate::workspace) fn handle_app_lock_dialog_key(
        &mut self,
        event: &KeyDownEvent,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.app_lock.dialog.is_none() {
            return false;
        }
        match event.keystroke.key.as_str() {
            "escape" => self.close_app_lock_dialog(cx),
            "enter" | "return" => self.submit_app_lock_dialog(cx),
            _ => return false,
        }
        true
    }

    pub(in crate::workspace) fn render_app_lock_activity_icon(
        &self,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let tooltip = self.i18n.t(if self.app_lock.configured {
            "sidebar.tooltips.lock_app"
        } else {
            "sidebar.tooltips.setup_app_lock"
        });
        let button = oxideterm_gpui_ui::button::icon_button(
            &self.tokens,
            Self::render_lucide_icon(
                LucideIcon::Lock,
                self.tokens.metrics.activity_icon_glyph_size,
                rgb(theme.text),
            ),
            oxideterm_gpui_ui::button::IconButtonOptions {
                size: self.tokens.metrics.activity_icon_size,
                radius: oxideterm_gpui_ui::button::ButtonRadius::Md,
                hover_background: Some(rgb(theme.bg_hover)),
                idle_opacity: 1.0,
                ..oxideterm_gpui_ui::button::IconButtonOptions::compact(
                    self.tokens.metrics.activity_icon_size,
                )
            },
        );
        button
            .id("activity-app-lock")
            .relative()
            .mb(px(self.tokens.metrics.activity_icon_gap))
            .on_mouse_move(
                cx.listener(move |this, event: &MouseMoveEvent, _window, cx| {
                    this.queue_workspace_tooltip(
                        "activity-app-lock",
                        tooltip.clone(),
                        f32::from(event.position.x) + 12.0,
                        f32::from(event.position.y) + 16.0,
                        cx,
                    );
                }),
            )
            .on_hover(cx.listener(|this, hovered: &bool, _window, cx| {
                if !*hovered {
                    this.clear_workspace_tooltip("activity-app-lock", cx);
                }
            }))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _event, window, cx| {
                    this.activate_app_lock(window, cx);
                    cx.stop_propagation();
                }),
            )
            .into_any_element()
    }

    pub(in crate::workspace) fn render_app_lock_settings_card(
        &self,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let configured = self.app_lock.configured;
        let status_label = self.i18n.t(if configured {
            "settings_view.general.app_lock_configured"
        } else {
            "settings_view.general.app_lock_not_configured"
        });
        let status_color = if configured {
            self.tokens.ui.success
        } else {
            self.tokens.ui.text_muted
        };
        let mut actions = div().flex().flex_row().flex_wrap().gap(px(8.0));
        if configured {
            actions = actions
                .child(self.app_lock_settings_action_button(
                    "settings_view.general.app_lock_change_password",
                    ButtonVariant::Outline,
                    AppLockDialog::Change,
                    cx,
                ))
                .child(self.app_lock_settings_action_button(
                    "settings_view.general.app_lock_remove",
                    ButtonVariant::Destructive,
                    AppLockDialog::Remove,
                    cx,
                ));
        } else {
            actions = actions.child(self.app_lock_settings_action_button(
                "settings_view.general.app_lock_set_password",
                ButtonVariant::Default,
                AppLockDialog::Configure,
                cx,
            ));
        }

        self.plain_settings_card(vec![
            self.card_title("settings_view.general.app_lock_title"),
            div()
                .w_full()
                .flex()
                .flex_row()
                .flex_wrap()
                .items_center()
                .justify_between()
                .gap(px(16.0))
                .child(
                    div()
                        .flex_1()
                        .min_w(px(240.0))
                        .flex()
                        .flex_col()
                        .gap(px(6.0))
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap(px(8.0))
                                .child(Self::render_lucide_icon(
                                    LucideIcon::Lock,
                                    16.0,
                                    rgb(self.tokens.ui.text),
                                ))
                                .child(
                                    div()
                                        .text_size(px(self.tokens.metrics.ui_text_sm))
                                        .font_weight(gpui::FontWeight::MEDIUM)
                                        .child(self.i18n.t("settings_view.general.app_lock_label")),
                                )
                                .child(self.text_badge(status_label, status_color)),
                        )
                        .child(
                            div()
                                .text_size(px(self.tokens.metrics.ui_text_xs))
                                .text_color(rgb(self.tokens.ui.text_muted))
                                .child(self.i18n.t("settings_view.general.app_lock_description")),
                        ),
                )
                .child(actions)
                .into_any_element(),
            self.general_checkbox_row(
                "settings_view.general.app_lock_show_sidebar_icon",
                "settings_view.general.app_lock_show_sidebar_icon_hint",
                self.settings_store.settings().sidebar_ui.show_app_lock_icon,
                |settings, enabled| settings.sidebar_ui.show_app_lock_icon = enabled,
                cx,
            ),
            self.app_lock
                .error
                .clone()
                .map(|error| self.app_lock_error_message(error))
                .unwrap_or_else(|| div().into_any_element()),
        ])
    }

    fn app_lock_settings_action_button(
        &self,
        label_key: &'static str,
        variant: ButtonVariant,
        dialog: AppLockDialog,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        self.workspace_toolbar_action_button(
            self.i18n.t(label_key),
            None,
            ToolbarButtonOptions {
                button: ButtonOptions {
                    variant,
                    size: ButtonSize::Sm,
                    radius: ButtonRadius::Md,
                    disabled: self.app_lock.pending,
                },
                ..ToolbarButtonOptions::default()
            },
            cx.listener(move |this, _event, window, cx| {
                this.open_app_lock_dialog(dialog, false, window, cx);
                cx.stop_propagation();
            }),
        )
        .into_any_element()
    }

    pub(in crate::workspace) fn render_app_lock_dialog(
        &self,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let dialog = self.app_lock.dialog?;
        let (title_key, description_key, submit_key) = match dialog {
            AppLockDialog::Configure => (
                "settings_view.general.app_lock_setup_title",
                "settings_view.general.app_lock_setup_description",
                "settings_view.general.app_lock_set_password",
            ),
            AppLockDialog::Change => (
                "settings_view.general.app_lock_change_title",
                "settings_view.general.app_lock_change_description",
                "settings_view.general.app_lock_change_password",
            ),
            AppLockDialog::Remove => (
                "settings_view.general.app_lock_remove_title",
                "settings_view.general.app_lock_remove_description",
                "settings_view.general.app_lock_remove",
            ),
        };
        let pending = self.app_lock.pending;
        let backdrop = dismissible_dialog_backdrop().on_mouse_down(
            MouseButton::Left,
            cx.listener(|this, _event, _window, cx| {
                this.close_app_lock_dialog(cx);
                cx.stop_propagation();
            }),
        );
        let mut fields = div()
            .px(px(24.0))
            .py(px(18.0))
            .flex()
            .flex_col()
            .gap(px(12.0));
        if matches!(dialog, AppLockDialog::Change | AppLockDialog::Remove) {
            fields = fields.child(self.portable_password_field(
                "settings_view.general.app_lock_current_password",
                SettingsInput::AppLockCurrentPassword,
                &self.app_lock.current_password,
                cx,
            ));
        }
        if matches!(dialog, AppLockDialog::Configure | AppLockDialog::Change) {
            fields = fields
                .child(self.portable_password_field(
                    "settings_view.general.app_lock_new_password",
                    SettingsInput::AppLockNewPassword,
                    &self.app_lock.new_password,
                    cx,
                ))
                .child(self.portable_password_field(
                    "settings_view.general.app_lock_confirm_password",
                    SettingsInput::AppLockConfirmPassword,
                    &self.app_lock.confirm_password,
                    cx,
                ));
        }
        fields = fields.when_some(self.app_lock.error.clone(), |fields, error| {
            fields.child(self.app_lock_error_message(error))
        });

        let form = dialog_content(&self.tokens)
            .w(px(APP_LOCK_DIALOG_WIDTH))
            .max_w(relative(0.92))
            .shadow_lg()
            .on_mouse_down(MouseButton::Left, |_event, _window, cx| {
                cx.stop_propagation();
            })
            .child(
                dialog_header(&self.tokens)
                    .child(dialog_title(&self.tokens, self.i18n.t(title_key)))
                    .child(dialog_description(
                        &self.tokens,
                        self.i18n.t(description_key),
                    )),
            )
            .child(fields)
            .child(
                dialog_footer(&self.tokens)
                    .child(self.standard_footer_action_button(
                        self.i18n.t("common.actions.cancel"),
                        ButtonVariant::Outline,
                        ConfirmDialogAction::Cancel,
                        pending,
                        |this, _event, _window, cx| this.close_app_lock_dialog(cx),
                        cx,
                    ))
                    .child(self.standard_footer_action_button(
                        if pending {
                            self.i18n.t("settings_view.general.app_lock_pending")
                        } else {
                            self.i18n.t(submit_key)
                        },
                        if dialog == AppLockDialog::Remove {
                            ButtonVariant::Destructive
                        } else {
                            ButtonVariant::Default
                        },
                        ConfirmDialogAction::Confirm,
                        pending,
                        |this, _event, _window, cx| this.submit_app_lock_dialog(cx),
                        cx,
                    )),
            );
        Some(
            div()
                .absolute()
                .top_0()
                .left_0()
                .right_0()
                .bottom_0()
                .child(backdrop)
                .child(
                    div()
                        .absolute()
                        .top_0()
                        .left_0()
                        .right_0()
                        .bottom_0()
                        .flex()
                        .items_center()
                        .justify_center()
                        .child(form),
                )
                .into_any_element(),
        )
    }

    pub(in crate::workspace) fn render_app_lock_screen(
        &self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let titlebar_visible = !window.is_fullscreen();
        let blocked = self.app_lock.retry_blocked();
        let can_submit = !self.app_lock.pending && !blocked;
        let card = oxideterm_gpui_ui::theme_card_surface_shadow(
            div()
                .w(px(APP_LOCK_SCREEN_CARD_WIDTH))
                .max_w(relative(0.9))
                .rounded(px(self.tokens.radii.lg))
                .border_1()
                .border_color(rgb(self.tokens.ui.border))
                .bg(rgb(self.tokens.ui.bg_panel))
                .p(px(28.0))
                .flex()
                .flex_col()
                .items_center()
                .gap(px(16.0))
                .child(
                    div()
                        .size(px(54.0))
                        .rounded_full()
                        .flex()
                        .items_center()
                        .justify_center()
                        .bg(rgba((self.tokens.ui.accent << 8) | 0x1f))
                        .child(Self::render_lucide_icon(
                            LucideIcon::Lock,
                            25.0,
                            rgb(self.tokens.ui.accent),
                        )),
                )
                .child(
                    div()
                        .text_size(px(22.0))
                        .font_weight(gpui::FontWeight::SEMIBOLD)
                        .text_color(rgb(self.tokens.ui.text))
                        .child(self.i18n.t("settings_view.general.app_lock_locked_title")),
                )
                .child(
                    div()
                        .max_w(px(APP_LOCK_SCREEN_DESCRIPTION_WIDTH))
                        .text_center()
                        .text_size(px(self.tokens.metrics.ui_text_sm))
                        .text_color(rgb(self.tokens.ui.text_muted))
                        .child(
                            self.i18n
                                .t("settings_view.general.app_lock_locked_description"),
                        ),
                )
                .child(
                    div()
                        .w_full()
                        .flex()
                        .flex_col()
                        .gap(px(8.0))
                        .child(
                            div()
                                .text_size(px(self.tokens.metrics.ui_text_sm))
                                .font_weight(gpui::FontWeight::MEDIUM)
                                .child(
                                    self.i18n
                                        .t("settings_view.general.app_lock_current_password"),
                                ),
                        )
                        .child(self.portable_password_input(
                            SettingsInput::AppLockCurrentPassword,
                            &self.app_lock.current_password,
                            cx,
                        )),
                )
                .when_some(self.app_lock.error.clone(), |card, error| {
                    card.child(self.app_lock_error_message(error))
                })
                .child(
                    div()
                        .flex()
                        .flex_wrap()
                        .items_center()
                        .justify_center()
                        .gap(px(10.0))
                        .when(self.app_lock.biometric_available, |actions| {
                            actions.child(self.standard_footer_action_button(
                                if self.app_lock.pending {
                                    self.i18n
                                        .t("settings_view.general.app_lock_biometric_verifying")
                                } else {
                                    self.i18n.t(if cfg!(target_os = "windows") {
                                        "settings_view.general.app_lock_windows_hello_unlock"
                                    } else {
                                        "settings_view.general.app_lock_biometric_unlock"
                                    })
                                },
                                ButtonVariant::Outline,
                                ConfirmDialogAction::Cancel,
                                self.app_lock.pending,
                                |this, _event, window, cx| this.submit_biometric_unlock(window, cx),
                                cx,
                            ))
                        })
                        .child(self.standard_footer_action_button(
                            if self.app_lock.pending {
                                self.i18n.t("settings_view.general.app_lock_unlocking")
                            } else if blocked {
                                self.i18n
                                    .t("settings_view.general.app_lock_wait_before_retry")
                            } else {
                                self.i18n.t("settings_view.general.app_lock_unlock")
                            },
                            ButtonVariant::Default,
                            ConfirmDialogAction::Confirm,
                            !can_submit,
                            |this, _event, _window, cx| this.submit_app_unlock(cx),
                            cx,
                        )),
                ),
            &self.tokens,
        );

        div()
            .id("app-lock-root")
            .size_full()
            .relative()
            .flex()
            .flex_col()
            // The lock surface stays opaque even when a workspace image or vibrancy is active.
            .bg(rgb(self.tokens.ui.bg))
            .text_color(rgb(self.tokens.ui.text))
            .font_family(settings_ui_font_family(
                &self.settings_store.settings().appearance.ui_font_family,
            ))
            .track_focus(&self.focus_handle)
            .key_context("Workspace")
            .capture_key_down(cx.listener(|this, event: &KeyDownEvent, window, cx| {
                this.handle_app_lock_key(event, window, cx);
            }))
            .when(titlebar_visible, |root| {
                root.child(self.render_title_bar(window, cx))
            })
            .child(
                div()
                    .flex_1()
                    .min_h(px(0.0))
                    .flex()
                    .items_center()
                    .justify_center()
                    .p(px(24.0))
                    .child(card),
            )
            .child(WorkspaceImeElement::new(
                cx.entity(),
                self.focus_handle.clone(),
            ))
            .into_any_element()
    }

    fn app_lock_error_message(&self, error: String) -> AnyElement {
        let message = if error.starts_with("settings_view.") {
            self.i18n.t(&error)
        } else {
            error
        };
        div()
            .w_full()
            .rounded(px(self.tokens.radii.md))
            .border_1()
            .border_color(rgba((self.tokens.ui.error << 8) | 0x4d))
            .bg(rgba((self.tokens.ui.error << 8) | 0x1a))
            .px(px(10.0))
            .py(px(8.0))
            .text_size(px(self.tokens.metrics.ui_text_sm))
            .text_color(rgb(self.tokens.ui.error))
            .child(message)
            .into_any_element()
    }
}

#[cfg(target_os = "windows")]
fn app_lock_native_window_handle(window: &Window) -> Option<isize> {
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};

    let handle = HasWindowHandle::window_handle(window).ok()?;
    let RawWindowHandle::Win32(handle) = handle.as_raw() else {
        return None;
    };
    Some(handle.hwnd.get())
}

#[cfg(not(target_os = "windows"))]
fn app_lock_native_window_handle(_window: &Window) -> Option<isize> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn repeated_failures_switch_to_extended_cooldown() {
        assert_eq!(app_lock_failure_cooldown(4), APP_LOCK_FAILURE_COOLDOWN);
        assert_eq!(app_lock_failure_cooldown(5), APP_LOCK_EXTENDED_COOLDOWN);
        assert_eq!(app_lock_failure_cooldown(20), APP_LOCK_EXTENDED_COOLDOWN);
    }
}
