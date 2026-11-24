use super::*;

#[path = "app_lock_visual.rs"]
mod visual;

use oxideterm_app_lock::{
    AppLockStore, BiometricAvailability, BiometricOutcome, authenticate_biometric,
    biometric_availability,
};
use oxideterm_gpui_settings_view::SettingsInput;
use oxideterm_gpui_ui::{
    button::{ButtonOptions, ButtonRadius, ButtonSize, ButtonVariant, ToolbarButtonOptions},
    modal::{
        TauriBackdropRole, dialog_content, dialog_description, dialog_footer, dialog_header,
        dialog_title, overlay_content_boundary, staged_dialog_overlay, staged_overlay_root,
    },
};
use zeroize::{Zeroize, Zeroizing};

const APP_LOCK_DIALOG_WIDTH: f32 = 460.0;
const APP_LOCK_CONSOLE_WIDE_WIDTH: f32 = 780.0;
const APP_LOCK_CONSOLE_NARROW_WIDTH: f32 = 420.0;
const APP_LOCK_CONSOLE_WIDE_BREAKPOINT: f32 = 780.0;
const APP_LOCK_CHAMBER_WIDE_RATIO: f32 = 0.46;
const APP_LOCK_BRAND_ICON_SIZE: f32 = 22.0;
const APP_LOCK_MINIMUM_PASSWORD_LENGTH: usize = 6;
const APP_LOCK_MARKER: &str = "app-locked";
const APP_LOCK_STATE_UNAVAILABLE: &str = "settings_view.general.app_lock_state_unavailable";
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

pub(super) use oxideterm_gpui_settings_view::app_lock_page::AppLockDialog;

pub(super) struct AppLockState {
    store: AppLockStore,
    marker_path: PathBuf,
    pub(super) configured: bool,
    pub(super) locked: bool,
    unlock_presence: oxideterm_gpui_ui::motion::ExitPresence,
    lock_enter_gen: u64,
    lock_anim_started: Option<Instant>,
    unlock_anim_started: Option<Instant>,
    error_shake_gen: u64,
    pub(super) unlock_exiting: bool,
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
    pub(super) fn load(store: AppLockStore, settings_path: &Path) -> Self {
        let (configured, mut error) = match store.is_configured() {
            Ok(configured) => (configured, None),
            Err(_) => (
                false,
                Some("settings_view.general.app_lock_unavailable".to_string()),
            ),
        };
        let marker_path = settings_path.with_file_name(APP_LOCK_MARKER);
        let locked = marker_path.try_exists().unwrap_or_else(|_| {
            // An unreadable marker cannot be treated as permission to unlock.
            error = Some(APP_LOCK_STATE_UNAVAILABLE.to_string());
            true
        });
        Self {
            store,
            marker_path,
            configured,
            locked,
            unlock_presence: oxideterm_gpui_ui::motion::ExitPresence::visible(),
            lock_enter_gen: 0,
            lock_anim_started: None,
            unlock_anim_started: None,
            error_shake_gen: 0,
            unlock_exiting: false,
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

    fn set_locked(&mut self, locked: bool) -> std::io::Result<()> {
        // The marker contains no credentials and must be durable before the UI
        // changes state, even if the process exits without running shutdown.
        if locked {
            oxideterm_atomic_file::durable_write(&self.marker_path, b"")?;
        } else {
            oxideterm_atomic_file::durable_remove(&self.marker_path)?;
        }
        self.locked = locked;
        Ok(())
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

    fn take_input_secret(&mut self, input: SettingsInput) -> Option<String> {
        let target = match input {
            SettingsInput::AppLockCurrentPassword => &mut self.current_password,
            SettingsInput::AppLockNewPassword => &mut self.new_password,
            SettingsInput::AppLockConfirmPassword => &mut self.confirm_password,
            _ => return None,
        };
        Some(std::mem::take(target))
    }

    fn replace_input_secret(&mut self, input: SettingsInput, value: String) -> bool {
        let target = match input {
            SettingsInput::AppLockCurrentPassword => &mut self.current_password,
            SettingsInput::AppLockNewPassword => &mut self.new_password,
            SettingsInput::AppLockConfirmPassword => &mut self.confirm_password,
            _ => return false,
        };
        // Replace the previous allocation only after scrubbing its contents.
        target.zeroize();
        *target = value;
        self.error = None;
        true
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
        if !self.begin_app_lock(cx) {
            cx.notify();
            return;
        }

        // Locking releases forwarded input before sensitive surfaces stop rendering,
        // preventing a held key or mouse button from remaining active remotely.
        self.release_active_remote_desktop_inputs(cx);
        self.finish_rail_resize(cx);
        self.finish_embedded_sftp_sidebar_resize(cx);
        self.finish_sftp_resize(cx);
        self.finish_terminal_command_sender_resize(cx);
        // A locked workspace must not keep sending unattended terminal input.
        self.terminal_command_sender
            .update(cx, |sender, cx| sender.stop_all(cx));
        self.suspend_public_mcp_runtime(cx);
        self.finish_split_drag(cx);
        self.close_terminal_command_overlays(cx);
        self.clear_workspace_tooltip("activity-app-lock", cx);
        self.clear_app_lock_input_state();
        self.app_lock.error = None;
        self.focus_app_lock_screen(window, cx);
        // Detached tab windows share the workspace lock state but own separate render roots.
        cx.refresh_windows();
    }

    fn persist_app_lock(&mut self, locked: bool, cx: &App) -> bool {
        {
            let admission = self.public_mcp.admission();
            let mut suspended = admission.lock();
            if self.app_lock.set_locked(locked).is_ok() {
                *suspended = locked;
                return true;
            }
        }
        self.app_lock.error = Some(APP_LOCK_STATE_UNAVAILABLE.to_string());
        if locked {
            self.push_workspace_notice(
                TerminalNotice {
                    title: self.i18n.t(APP_LOCK_STATE_UNAVAILABLE),
                    description: None,
                    status_text: None,
                    progress: None,
                    variant: TerminalNoticeVariant::Warning,
                },
                cx,
            );
        }
        false
    }

    fn begin_app_lock(&mut self, cx: &App) -> bool {
        let was_locked = self.app_lock.locked;
        let audit = oxideterm_audit::AuditOperation::begin(
            oxideterm_audit::AuditCategory::Security,
            "app_lock",
            None,
            None,
        );
        let persisted = self.persist_app_lock(true, cx);
        if persisted && !was_locked {
            self.app_lock.unlock_exiting = false;
            self.app_lock.unlock_anim_started = None;
            self.app_lock.unlock_presence.reopen();
            self.app_lock.lock_enter_gen = self.app_lock.lock_enter_gen.wrapping_add(1);
            self.app_lock.lock_anim_started = (self.tokens.motion.enabled
                && self.tokens.motion.spatial_enabled)
                .then(Instant::now);
            self.app_lock.error_shake_gen = 0;
        }
        audit.finish(
            if !persisted {
                oxideterm_audit::AuditOutcome::Failed
            } else if was_locked {
                oxideterm_audit::AuditOutcome::Unchanged
            } else {
                oxideterm_audit::AuditOutcome::Succeeded
            },
            oxideterm_audit::AuditEvidence::Lifecycle,
            None,
            None,
        );
        persisted
    }

    pub(super) fn focus_app_lock_screen(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.refresh_app_lock_biometric_availability(cx);
        self.focus_app_lock_input(SettingsInput::AppLockCurrentPassword, window, cx);
        window.set_window_title(&SharedString::from(
            self.i18n.t("settings_view.general.app_lock_window_title"),
        ));
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
        self.focus_settings_input(input, String::new(), cx);
        window.focus(&self.focus_handle, cx);
    }

    pub(in crate::workspace) fn take_app_lock_input_value(
        &mut self,
        input: SettingsInput,
    ) -> Option<String> {
        self.app_lock.take_input_secret(input)
    }

    fn replace_app_lock_input_value(&mut self, input: SettingsInput, value: String) -> bool {
        self.app_lock.replace_input_secret(input, value)
    }

    pub(in crate::workspace) fn commit_focused_app_lock_input(&mut self) {
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
        let draft = std::mem::take(&mut self.settings_input_draft);
        let _ = self.replace_app_lock_input_value(input, draft);
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
        let audit = oxideterm_audit::AuditOperation::begin(
            oxideterm_audit::AuditCategory::Security,
            match dialog {
                AppLockDialog::Configure => "app_lock_setup",
                AppLockDialog::Change => "app_lock_change",
                AppLockDialog::Remove => "app_lock_remove",
            },
            None,
            None,
        );
        cx.spawn(async move |weak, cx| {
            let task = runtime.spawn_blocking(move || {
                let result = match dialog {
                    AppLockDialog::Configure => store.set_password(new_password).map(|_| true),
                    AppLockDialog::Change => store.change_password(current_password, new_password),
                    AppLockDialog::Remove => store.remove_password(current_password),
                };
                audit.finish(
                    match &result {
                        Ok(true) => oxideterm_audit::AuditOutcome::Succeeded,
                        Ok(false) => oxideterm_audit::AuditOutcome::Denied,
                        Err(_) => oxideterm_audit::AuditOutcome::Failed,
                    },
                    oxideterm_audit::AuditEvidence::Protocol,
                    None,
                    None,
                );
                result
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
                        if should_lock && this.begin_app_lock(cx) {
                            this.suspend_public_mcp_runtime(cx);
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
            self.app_lock.error_shake_gen = self.app_lock.error_shake_gen.wrapping_add(1);
            cx.notify();
            return;
        }

        let password = Zeroizing::new(std::mem::take(&mut self.app_lock.current_password));
        let store = self.app_lock.store.clone();
        let runtime = self.forwarding_runtime.handle().clone();
        let audit = oxideterm_audit::AuditOperation::begin(
            oxideterm_audit::AuditCategory::Security,
            "app_unlock",
            None,
            Some("password"),
        );
        self.app_lock.pending = true;
        self.app_lock.error = None;
        cx.spawn(async move |weak, cx| {
            let task = runtime.spawn_blocking(move || {
                let result = store.verify_password(password);
                audit.finish(
                    match &result {
                        Ok(true) => oxideterm_audit::AuditOutcome::Succeeded,
                        Ok(false) => oxideterm_audit::AuditOutcome::Denied,
                        Err(_) => oxideterm_audit::AuditOutcome::Failed,
                    },
                    oxideterm_audit::AuditEvidence::Protocol,
                    None,
                    None,
                );
                result
            });
            let result = task
                .await
                .map_err(|error| error.to_string())
                .and_then(|result| result.map_err(|error| error.to_string()));
            let _ = weak.update(cx, |this, cx| {
                this.app_lock.pending = false;
                match result {
                    Ok(true) => this.finish_app_unlock(cx),
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
        let audit = oxideterm_audit::AuditOperation::begin(
            oxideterm_audit::AuditCategory::Security,
            "app_unlock",
            None,
            Some("biometric"),
        );
        self.app_lock.pending = true;
        self.app_lock.error = None;
        cx.spawn(async move |weak, cx| {
            let task = runtime.spawn_blocking(move || {
                let result = authenticate_biometric(&reason, native_window_handle);
                audit.finish(
                    match &result {
                        Ok(BiometricOutcome::Verified) => oxideterm_audit::AuditOutcome::Succeeded,
                        Ok(BiometricOutcome::Canceled) => oxideterm_audit::AuditOutcome::Cancelled,
                        Ok(BiometricOutcome::Failed | BiometricOutcome::Unavailable) => {
                            oxideterm_audit::AuditOutcome::Denied
                        }
                        Err(_) => oxideterm_audit::AuditOutcome::Failed,
                    },
                    oxideterm_audit::AuditEvidence::Protocol,
                    None,
                    None,
                );
                result
            });
            let result = task
                .await
                .map_err(|error| error.to_string())
                .and_then(|result| result.map_err(|error| error.to_string()));
            let _ = weak.update(cx, |this, cx| {
                this.app_lock.pending = false;
                match result {
                    Ok(BiometricOutcome::Verified) => this.finish_app_unlock(cx),
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

    fn finish_app_unlock(&mut self, cx: &mut Context<Self>) {
        if !self.persist_app_lock(false, cx) {
            return;
        }
        self.app_lock.failed_attempts = 0;
        self.app_lock.retry_at = None;
        self.app_lock.lock_anim_started = None;
        self.clear_app_lock_input_state();
        self.needs_active_pane_focus = true;
        if self.tokens.motion.enabled
            && let Some(generation) = self.app_lock.unlock_presence.begin_exit()
        {
            self.app_lock.unlock_exiting = true;
            self.app_lock.unlock_anim_started = Some(Instant::now());
            let delay = if self.tokens.motion.spatial_enabled {
                oxideterm_gpui_ui::motion::scaled_duration(
                    &self.tokens,
                    visual::APP_LOCK_EXIT_BASE_MS,
                ) + Duration::from_millis(60)
            } else {
                oxideterm_gpui_ui::motion::exit_settle_duration(
                    &self.tokens,
                    oxideterm_gpui_ui::motion::MotionDuration::Control,
                )
            };
            cx.spawn(async move |weak, cx| {
                cx.background_executor().timer(delay).await;
                let _ = weak.update(cx, |this, cx| {
                    if this.app_lock.unlock_presence.finish_exit(generation) {
                        this.app_lock.unlock_exiting = false;
                        this.app_lock.unlock_anim_started = None;
                        cx.notify();
                    }
                });
            })
            .detach();
        } else {
            self.app_lock.unlock_exiting = false;
            self.app_lock.unlock_anim_started = None;
        }
        cx.refresh_windows();
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
        self.app_lock.error_shake_gen = self.app_lock.error_shake_gen.wrapping_add(1);
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

    pub(in crate::workspace) fn render_app_lock_button(
        &self,
        button_size: f32,
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
                size: button_size,
                radius: oxideterm_gpui_ui::button::ButtonRadius::Md,
                hover_background: Some(rgb(theme.bg_hover)),
                idle_opacity: 1.0,
                ..oxideterm_gpui_ui::button::IconButtonOptions::compact(button_size)
            },
        );
        button
            .id("activity-app-lock")
            .relative()
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

    pub(super) fn settings_app_lock_state(&self) -> (bool, bool, Option<String>) {
        (
            self.app_lock.configured,
            self.app_lock.pending,
            self.app_lock.error.clone(),
        )
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
        Some(staged_dialog_overlay(
            &self.tokens,
            ("app-lock-dialog", dialog as usize),
            TauriBackdropRole::Dialog,
            staged_overlay_root().on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _event, _window, cx| {
                    this.close_app_lock_dialog(cx);
                    cx.stop_propagation();
                }),
            ),
            overlay_content_boundary(form),
            8.0,
            oxideterm_gpui_ui::motion::MotionDuration::Control,
            true,
        ))
    }

    fn render_app_lock_hero(
        &self,
        interactive: bool,
        wide: bool,
        padlock: visual::PadlockVisual,
        hide_chamber_padlock: bool,
        cx: &mut Context<Self>,
    ) -> gpui::Div {
        let blocked = self.app_lock.retry_blocked();
        let can_submit = interactive && !self.app_lock.pending && !blocked;
        let description_key = if self.knowledge_state().read(cx).app_quit_pending() {
            "menu.unlock_to_finish_quit"
        } else {
            "settings_view.general.app_lock_locked_description"
        };
        let icon_source = crate::app_icon::app_icon_trimmed_source_sized(
            self.settings_store.settings().appearance.app_icon,
            APP_LOCK_BRAND_ICON_SIZE,
        );
        let action_height = 36.0;
        let unlock_button = self
            .standard_footer_action_button(
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
            )
            .w_full()
            .h(px(action_height))
            .justify_center();

        let biometric_button = (interactive && self.app_lock.biometric_available).then(|| {
            let group_id: SharedString = "app-lock-biometric-action".into();
            let idle_color = if self.app_lock.pending {
                self.tokens.ui.text_muted
            } else {
                self.tokens.ui.text_secondary
            };
            let hover_color = self.tokens.ui.text;
            let mut icon = svg()
                .path(LucideIcon::ShieldCheck.path())
                .size(px(14.0))
                .text_color(rgb(idle_color));
            if !self.app_lock.pending {
                icon = icon.group_hover(group_id.clone(), move |style| {
                    style.text_color(rgb(hover_color))
                });
            }
            self.workspace_toolbar_action_button(
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
                Some(icon.into_any_element()),
                ToolbarButtonOptions {
                    button: ButtonOptions {
                        variant: ButtonVariant::Outline,
                        size: ButtonSize::Default,
                        radius: ButtonRadius::Md,
                        disabled: self.app_lock.pending,
                    },
                    height: Some(action_height),
                    padding_x: Some(14.0),
                    font_size: Some(self.tokens.metrics.ui_text_sm),
                    text_color: Some(rgb(idle_color).into()),
                    hover_text_color: (!self.app_lock.pending).then(|| rgb(hover_color).into()),
                    ..ToolbarButtonOptions::default()
                },
                cx.listener(|this, _event, window, cx| {
                    this.submit_biometric_unlock(window, cx);
                    cx.stop_propagation();
                }),
            )
            .w_full()
            .justify_center()
            .id(group_id.clone())
            .group(group_id)
        });

        let padlock_size = if wide {
            visual::APP_LOCK_CHAMBER_SIZE_WIDE
        } else {
            visual::APP_LOCK_CHAMBER_SIZE_NARROW
        };
        let chamber = div()
            .bg(rgb(self.tokens.ui.bg_panel))
            .when(wide, |chamber| {
                chamber
                    .w(relative(APP_LOCK_CHAMBER_WIDE_RATIO))
                    .flex_none()
                    .border_r_1()
                    .border_color(rgb(self.tokens.ui.border))
            })
            .when(!wide, |chamber| {
                chamber
                    .w_full()
                    .border_b_1()
                    .border_color(rgb(self.tokens.ui.border))
            })
            .px(px(28.0))
            .py(px(28.0))
            .flex()
            .flex_col()
            .justify_between()
            .gap(px(16.0))
            .child(
                div()
                    .flex()
                    .items_center()
                    .when(!wide, |row| row.justify_center())
                    .gap(px(8.0))
                    .child(
                        gpui::img(icon_source)
                            .size(px(APP_LOCK_BRAND_ICON_SIZE))
                            .flex_none()
                            .object_fit(gpui::ObjectFit::Contain),
                    )
                    .child(
                        div()
                            .text_size(px(14.0))
                            .line_height(px(20.0))
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .text_color(rgb(self.tokens.ui.text_heading))
                            .child("OxideTerm"),
                    ),
            )
            .child(
                div()
                    .flex_1()
                    .flex()
                    .items_center()
                    .justify_center()
                    .py(px(4.0))
                    .when(hide_chamber_padlock, |slot| slot.opacity(0.0))
                    .child(visual::render_padlock_mechanism(
                        self.tokens.ui,
                        padlock_size,
                        padlock,
                    )),
            )
            .child(
                div()
                    .when(interactive, |desc| {
                        desc.debug_selector(move || description_key.into())
                    })
                    .w_full()
                    .when(!wide, |desc| desc.text_center())
                    .text_size(px(self.tokens.metrics.ui_text_xs))
                    .line_height(px(18.0))
                    .text_color(rgb(self.tokens.ui.text_muted))
                    .child(self.i18n.t(description_key)),
            );

        let form_panel = div()
            .flex_1()
            .min_w(px(0.0))
            .bg(rgb(self.tokens.ui.bg_card))
            .px(px(if wide { 40.0 } else { 28.0 }))
            .py(px(if wide { 36.0 } else { 26.0 }))
            .flex()
            .flex_col()
            .justify_center()
            .gap(px(20.0))
            .child(
                div()
                    .text_size(px(20.0))
                    .line_height(px(26.0))
                    .font_weight(gpui::FontWeight::SEMIBOLD)
                    .text_color(rgb(self.tokens.ui.text_heading))
                    .child(self.i18n.t("settings_view.general.app_lock_locked_title")),
            )
            .child(
                div()
                    .w_full()
                    .flex()
                    .flex_col()
                    .gap(px(12.0))
                    .child(
                        div().w_full().child(
                            self.portable_password_input_with_placeholder(
                                SettingsInput::AppLockCurrentPassword,
                                if interactive {
                                    &self.app_lock.current_password
                                } else {
                                    ""
                                },
                                self.i18n
                                    .t("settings_view.general.app_lock_unlock_placeholder"),
                                cx,
                            ),
                        ),
                    )
                    .when_some(
                        interactive.then(|| self.app_lock.error.clone()).flatten(),
                        |form, error| form.child(self.app_lock_error_message(error)),
                    )
                    .child(unlock_button),
            )
            .when_some(biometric_button, |panel, button| {
                panel
                    .child(div().w_full().h(px(1.0)).bg(rgb(self.tokens.ui.border)))
                    .child(button)
            });

        let light = self.tokens.ui.is_light();
        let console_shadows = vec![
            gpui::BoxShadow {
                color: gpui::rgba(if light { 0x00000014 } else { 0x0000005c }).into(),
                offset: gpui::point(px(0.0), px(22.0)),
                blur_radius: px(48.0),
                spread_radius: px(-8.0),
                inset: false,
            },
            gpui::BoxShadow {
                color: gpui::rgba(if light { 0x0000000c } else { 0x00000038 }).into(),
                offset: gpui::point(px(0.0), px(6.0)),
                blur_radius: px(16.0),
                spread_radius: px(-2.0),
                inset: false,
            },
            gpui::BoxShadow {
                color: gpui::rgba(if light { 0xffffff80 } else { 0xffffff12 }).into(),
                offset: gpui::point(px(0.0), px(1.0)),
                blur_radius: px(0.0),
                spread_radius: px(0.0),
                inset: true,
            },
        ];

        div()
            .w_full()
            .max_w(px(if wide {
                APP_LOCK_CONSOLE_WIDE_WIDTH
            } else {
                APP_LOCK_CONSOLE_NARROW_WIDTH
            }))
            .when(wide, |card| card.min_h(px(412.0)))
            .rounded(px(self.tokens.radii.lg + 6.0))
            .bg(rgb(self.tokens.ui.bg_card))
            .border_1()
            .border_color(rgb(self.tokens.ui.border))
            .shadow(console_shadows)
            .overflow_hidden()
            .flex()
            .when(wide, |card| card.flex_row())
            .when(!wide, |card| card.flex_col())
            .child(chamber)
            .child(form_panel)
    }

    pub(in crate::workspace) fn render_app_lock_screen(
        &self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let titlebar_visible = self.window_titlebar_visible(window);
        let viewport_w = f32::from(window.viewport_size().width);
        let wide = viewport_w >= APP_LOCK_CONSOLE_WIDE_BREAKPOINT;
        let generation = self.app_lock.lock_enter_gen;
        let chamber_padlock_size = if wide {
            visual::APP_LOCK_CHAMBER_SIZE_WIDE
        } else {
            visual::APP_LOCK_CHAMBER_SIZE_NARROW
        };
        let target_scale = chamber_padlock_size / visual::APP_LOCK_RITUAL_SIZE;

        let enter_frame = if self.tokens.motion.enabled && self.tokens.motion.spatial_enabled {
            self.app_lock.lock_anim_started.and_then(|started| {
                let total = oxideterm_gpui_ui::motion::scaled_duration(
                    &self.tokens,
                    visual::APP_LOCK_ENTER_BASE_MS,
                );
                let t = if total.is_zero() {
                    1.0
                } else {
                    (started.elapsed().as_secs_f32() / total.as_secs_f32()).clamp(0.0, 1.0)
                };
                if t < 1.0 {
                    window.request_animation_frame();
                    Some(visual::app_lock_enter_frame(t, target_scale))
                } else {
                    None
                }
            })
        } else {
            None
        };

        let ritual_active = enter_frame.is_some_and(|enter| enter.ritual_visible);
        let padlock = if self.app_lock.error.is_some() {
            visual::PadlockVisual::settled_error()
        } else {
            visual::PadlockVisual::settled_locked()
        };
        let lock_hero = self.render_app_lock_hero(true, wide, padlock, ritual_active, cx);
        let console: AnyElement = if let Some(enter) = enter_frame {
            div()
                .w_full()
                .flex()
                .justify_center()
                .opacity(enter.screen_opacity)
                .relative()
                .top(px(enter.screen_offset_y))
                .child(lock_hero)
                .into_any_element()
        } else if self.app_lock.error_shake_gen > 0
            && self.tokens.motion.enabled
            && self.tokens.motion.spatial_enabled
        {
            let shake_duration = oxideterm_gpui_ui::motion::scaled_duration(&self.tokens, 280);
            div()
                .w_full()
                .flex()
                .justify_center()
                .child(lock_hero)
                .with_animation(
                    ("app-lock-shake", self.app_lock.error_shake_gen),
                    gpui::Animation::new(shake_duration),
                    |el, delta| {
                        let offset =
                            (delta * std::f32::consts::TAU * 3.0).sin() * (1.0 - delta) * 7.0;
                        el.relative().left(px(offset))
                    },
                )
                .into_any_element()
        } else if self.tokens.motion.enabled && !self.tokens.motion.spatial_enabled {
            oxideterm_gpui_ui::motion::fade_in(
                &self.tokens,
                ("app-lock-enter", generation),
                div().w_full().flex().justify_center().child(lock_hero),
                oxideterm_gpui_ui::motion::MotionDuration::Control,
            )
        } else {
            div()
                .w_full()
                .flex()
                .justify_center()
                .child(lock_hero)
                .into_any_element()
        };

        // Compute the resting center of the chamber padlock relative to the stage center
        // so the single ritual padlock glides directly into its slot without double-rendering.
        let console_w = (viewport_w - 64.0)
            .min(if wide {
                APP_LOCK_CONSOLE_WIDE_WIDTH
            } else {
                APP_LOCK_CONSOLE_NARROW_WIDTH
            })
            .max(0.0);
        let (target_dx, target_dy) = if wide {
            (-console_w * (0.5 - APP_LOCK_CHAMBER_WIDE_RATIO * 0.5), -6.0)
        } else {
            (0.0, -104.0)
        };

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
                if this.tab_host.read(cx).close_confirm() == Some(&TabCloseConfirm::ApplicationQuit)
                {
                    this.handle_tab_close_confirm_key(event, window, cx);
                    window.prevent_default();
                    cx.stop_propagation();
                } else {
                    this.handle_app_lock_key(event, window, cx);
                }
            }))
            .child(visual::render_lock_screen_backdrop(self.tokens.ui))
            .when(titlebar_visible, |root| {
                root.child(self.render_locked_title_bar(window, cx))
            })
            .child(
                div()
                    .flex_1()
                    .min_h(px(0.0))
                    .relative()
                    .flex()
                    .items_center()
                    .justify_center()
                    .px(px(32.0))
                    .py(px(28.0))
                    .child(console)
                    .when_some(
                        enter_frame.filter(|enter| enter.ritual_visible),
                        |stage, enter| {
                            let glide_x = target_dx * enter.handoff_progress;
                            let glide_y = target_dy * enter.handoff_progress;
                            stage.child(
                                div()
                                    .absolute()
                                    .inset_0()
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .opacity(enter.ritual_opacity)
                                    .child(
                                        div().relative().left(px(glide_x)).top(px(glide_y)).child(
                                            visual::render_padlock_mechanism(
                                                self.tokens.ui,
                                                visual::APP_LOCK_RITUAL_SIZE,
                                                enter.ritual_visual,
                                            ),
                                        ),
                                    ),
                            )
                        },
                    ),
            )
            .when(
                self.tab_host.read(cx).close_confirm() == Some(&TabCloseConfirm::ApplicationQuit),
                |root| root.child(self.render_tab_close_confirm_dialog(cx)),
            )
            .child(WorkspaceImeElement::new(
                cx.entity(),
                self.focus_handle.clone(),
                window.window_handle().window_id(),
            ))
            .into_any_element()
    }

    pub(in crate::workspace) fn render_app_lock_exit_overlay(
        &self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let titlebar_visible = self.window_titlebar_visible(window);
        let wide = f32::from(window.viewport_size().width) >= APP_LOCK_CONSOLE_WIDE_BREAKPOINT;
        let spatial = self.tokens.motion.spatial_enabled;
        let total = if spatial {
            oxideterm_gpui_ui::motion::scaled_duration(&self.tokens, visual::APP_LOCK_EXIT_BASE_MS)
        } else {
            oxideterm_gpui_ui::motion::exit_duration(
                &self.tokens,
                oxideterm_gpui_ui::motion::MotionDuration::Control,
            )
        };
        let t = match self.app_lock.unlock_anim_started {
            Some(started) if !total.is_zero() => {
                (started.elapsed().as_secs_f32() / total.as_secs_f32()).clamp(0.0, 1.0)
            }
            _ => 1.0,
        };
        if t < 1.0 {
            window.request_animation_frame();
        }
        let exit = visual::app_lock_exit_frame(t, spatial);
        let lock_hero = self.render_app_lock_hero(false, wide, exit.padlock, false, cx);

        div()
            .absolute()
            .inset_0()
            .opacity(exit.curtain_opacity)
            .child(
                div()
                    .size_full()
                    .relative()
                    .flex()
                    .flex_col()
                    .bg(rgb(self.tokens.ui.bg))
                    .text_color(rgb(self.tokens.ui.text))
                    .font_family(settings_ui_font_family(
                        &self.settings_store.settings().appearance.ui_font_family,
                    ))
                    .child(visual::render_lock_screen_backdrop(self.tokens.ui))
                    .when(titlebar_visible, |root| {
                        root.child(self.render_locked_title_bar(window, cx))
                    })
                    .child(
                        div()
                            .flex_1()
                            .min_h(px(0.0))
                            .relative()
                            .flex()
                            .items_center()
                            .justify_center()
                            .px(px(32.0))
                            .py(px(28.0))
                            .child(
                                div()
                                    .w_full()
                                    .flex()
                                    .justify_center()
                                    .relative()
                                    .top(px(exit.curtain_offset_y))
                                    .child(lock_hero),
                            ),
                    ),
            )
            .into_any_element()
    }

    fn app_lock_error_message(&self, error: String) -> AnyElement {
        oxideterm_gpui_settings_view::app_lock_page::render_app_lock_error(
            &self.tokens,
            &self.i18n,
            error,
        )
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
    fn app_lock_survives_a_new_workspace_until_unlocked() {
        let executable = std::env::current_exe().unwrap();
        let fixture_key = "OXIDETERM_APP_LOCK_TEST_DIR";
        let Some(root) = std::env::var_os(fixture_key) else {
            let directory = tempfile::tempdir_in(executable.parent().unwrap()).unwrap();
            let child = directory.path().join(executable.file_name().unwrap());
            std::fs::hard_link(executable, &child).unwrap();
            std::fs::write(directory.path().join("portable"), []).unwrap();
            let output = std::process::Command::new(child)
                .args([
                    "app_lock_survives_a_new_workspace_until_unlocked",
                    "--nocapture",
                ])
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

        oxideterm_portable_runtime::initialize_portable_runtime().unwrap();
        let settings = oxideterm_settings::default_settings_path();
        assert!(settings.starts_with(PathBuf::from(root)));
        oxideterm_portable_runtime::keystore::create_portable_keystore("test-only-vault-password")
            .unwrap();
        let store = AppLockStore::new();
        store
            .set_password(Zeroizing::new("test-only-app-password".into()))
            .unwrap();

        let mut state = AppLockState::load(store.clone(), &settings);
        assert!(state.configured);
        assert!(!state.locked, "configuring a password alone does not lock");
        state.set_locked(true).unwrap();
        assert_eq!(std::fs::read(&state.marker_path).unwrap(), b"");
        drop(state);

        let mut reopened = AppLockState::load(store.clone(), &settings);
        assert!(reopened.locked, "a new workspace must start locked");
        reopened.set_locked(false).unwrap();
        assert!(!AppLockState::load(store, &settings).locked);
    }

    #[test]
    fn repeated_failures_switch_to_extended_cooldown() {
        assert_eq!(app_lock_failure_cooldown(4), APP_LOCK_FAILURE_COOLDOWN);
        assert_eq!(app_lock_failure_cooldown(5), APP_LOCK_EXTENDED_COOLDOWN);
        assert_eq!(app_lock_failure_cooldown(20), APP_LOCK_EXTENDED_COOLDOWN);
    }

    #[test]
    fn focused_password_handoff_moves_one_allocation_between_owners() {
        let mut state = AppLockState {
            store: AppLockStore::new(),
            marker_path: PathBuf::new(),
            configured: true,
            locked: false,
            unlock_presence: oxideterm_gpui_ui::motion::ExitPresence::visible(),
            lock_enter_gen: 0,
            lock_anim_started: None,
            unlock_anim_started: None,
            error_shake_gen: 0,
            unlock_exiting: false,
            dialog: Some(AppLockDialog::Change),
            lock_after_configure: false,
            pending: false,
            biometric_available: false,
            biometric_check_pending: false,
            current_password: "current-secret".into(),
            new_password: String::new(),
            confirm_password: String::new(),
            error: Some("stale-error".into()),
            failed_attempts: 0,
            retry_at: None,
        };
        let allocation = state.current_password.as_ptr();

        let draft = state
            .take_input_secret(SettingsInput::AppLockCurrentPassword)
            .expect("app lock password input");

        assert_eq!(draft.as_ptr(), allocation);
        assert!(state.current_password.is_empty());
        assert!(state.replace_input_secret(SettingsInput::AppLockCurrentPassword, draft));
        assert_eq!(state.current_password.as_ptr(), allocation);
        assert!(state.error.is_none());
    }
}
