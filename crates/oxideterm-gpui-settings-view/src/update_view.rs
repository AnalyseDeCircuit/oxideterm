use std::rc::Rc;

use gpui::{
    AnyElement, App, Context, Entity, MouseButton, MouseDownEvent, Render, Subscription, Task,
    Window, div, prelude::*, px, relative, rgb,
};
use oxideterm_gpui_ui::{
    ConfirmDialogAction,
    button::{
        ButtonOptions, ButtonRadius, ButtonSize, ButtonVariant, ToolbarButtonOptions,
        toolbar_button,
    },
    modal::{
        dialog_content, dialog_description, dialog_footer, dialog_header, dialog_title,
        dismissible_dialog_backdrop, overlay_content_boundary,
    },
    motion::{self, ExitPhase, ExitPresence, MotionDuration},
    toast::{ToastVariant, ToastView, toast, toast_action, toast_close},
};
use oxideterm_i18n::I18n;
use oxideterm_theme::ThemeTokens;

use crate::{
    dialog::settings_dialog_transition,
    entity::{SettingsWorkspaceEntity, SettingsWorkspaceEvent},
    native_update::{
        NativeUpdateRenderState, native_update_progress_hint, native_update_progress_ratio,
    },
};

const NATIVE_UPDATE_RELEASE_NOTES_WIDTH: f32 = 760.0;
const NATIVE_UPDATE_RELEASE_NOTES_HEIGHT: f32 = 720.0;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NativeUpdateNotificationAction {
    Dismiss,
    ReleaseNotes,
    Download,
    Cancel,
    Install,
    Retry,
}

pub struct NativeUpdateNoticeFrame {
    pub tokens: ThemeTokens,
    pub i18n: I18n,
    pub portable: bool,
}

pub struct NativeUpdateNoticeBindings {
    pub frame: Rc<dyn Fn(&App) -> Option<NativeUpdateNoticeFrame>>,
    pub dispatch: Rc<dyn Fn(NativeUpdateNotificationAction, &mut App)>,
}

pub struct NativeUpdateNoticeView {
    settings: Entity<SettingsWorkspaceEntity>,
    bindings: NativeUpdateNoticeBindings,
    open: bool,
    presence: ExitPresence,
    exit_task: Option<Task<()>>,
    _settings_changed: Subscription,
    _settings_events: Subscription,
}

impl NativeUpdateNoticeView {
    pub(crate) fn new(
        settings: &Entity<SettingsWorkspaceEntity>,
        bindings: NativeUpdateNoticeBindings,
        cx: &mut Context<Self>,
    ) -> Self {
        let settings_changed = cx.observe(settings, |view, _, cx| {
            if view.open {
                cx.notify();
            }
        });
        let settings_events = cx.subscribe(settings, |view, _, event, cx| {
            match event {
                SettingsWorkspaceEvent::ResetNativeUpdateOverlay => view.open = false,
                SettingsWorkspaceEvent::ShowNativeUpdateNotification => view.open = true,
                _ => return,
            }
            view.exit_task = None;
            view.presence.reopen();
            cx.notify();
        });
        Self {
            settings: settings.clone(),
            bindings,
            open: false,
            presence: ExitPresence::visible(),
            exit_task: None,
            _settings_changed: settings_changed,
            _settings_events: settings_events,
        }
    }

    fn dismiss(&mut self, tokens: ThemeTokens, cx: &mut Context<Self>) {
        let Some(generation) = self.presence.begin_exit() else {
            return;
        };
        let delay = motion::duration(&tokens, MotionDuration::Control);
        if delay.is_zero() {
            self.open = false;
            self.presence.reopen();
        } else {
            self.exit_task = Some(cx.spawn(async move |view, cx| {
                cx.background_executor().timer(delay).await;
                let _ = view.update(cx, |view, cx| {
                    if view.presence.finish_exit(generation) {
                        view.open = false;
                        view.presence.reopen();
                        cx.notify();
                    }
                });
            }));
        }
        cx.notify();
    }
}

impl Render for NativeUpdateNoticeView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let Some(frame) = self.open.then(|| (self.bindings.frame)(cx)).flatten() else {
            return div().hidden().into_any_element();
        };
        let tokens = frame.tokens;
        let owner = cx.entity().downgrade();
        let host = self.bindings.dispatch.clone();
        let dispatch = Rc::new(move |action, cx: &mut App| {
            if action == NativeUpdateNotificationAction::Dismiss {
                let _ = owner.update(cx, |view, cx| view.dismiss(tokens, cx));
            } else {
                host(action, cx);
            }
        });
        let settings = self.settings.read(cx);
        NativeUpdateNotice {
            i18n: &frame.i18n,
            tokens: &tokens,
            state: settings.native_update_render_state(),
            phase: self.presence.phase(),
            description: settings.native_update_package_description(),
            has_release_notes: settings.native_update_has_release_notes(),
            portable: frame.portable,
            dispatch,
        }
        .render()
        .map(|notice| toast(&tokens, notice))
        .unwrap_or_else(|| div().hidden().into_any_element())
    }
}

pub struct NativeUpdateNotice<'a> {
    pub i18n: &'a I18n,
    pub tokens: &'a ThemeTokens,
    pub state: NativeUpdateRenderState,
    pub phase: ExitPhase,
    pub description: Option<String>,
    pub has_release_notes: bool,
    pub portable: bool,
    pub dispatch: Rc<dyn Fn(NativeUpdateNotificationAction, &mut App)>,
}

impl NativeUpdateNotice<'_> {
    fn action(
        &self,
        action: NativeUpdateNotificationAction,
    ) -> impl Fn(&MouseDownEvent, &mut Window, &mut App) + use<> {
        let dispatch = self.dispatch.clone();
        move |_, _, cx| {
            dispatch(action, cx);
            cx.stop_propagation();
        }
    }

    pub fn render(&self) -> Option<ToastView> {
        let (title, status_text, progress, variant) = match &self.state {
            NativeUpdateRenderState::Available { .. } => (
                self.i18n.t("settings_view.help.update_available"),
                None,
                None,
                ToastVariant::Default,
            ),
            NativeUpdateRenderState::Downloading(status) => (
                self.i18n.t("settings_view.help.downloading"),
                status.as_ref().map(native_update_progress_hint),
                status
                    .as_ref()
                    .and_then(native_update_progress_ratio)
                    .map(|ratio| ratio * 100.0)
                    .or(Some(0.0)),
                ToastVariant::Default,
            ),
            NativeUpdateRenderState::Verifying(status) => (
                self.i18n.t("settings_view.help.verifying"),
                status.as_ref().map(native_update_progress_hint),
                Some(100.0),
                ToastVariant::Default,
            ),
            NativeUpdateRenderState::Downloaded => (
                self.i18n.t("settings_view.help.update_downloaded"),
                None,
                None,
                ToastVariant::Success,
            ),
            NativeUpdateRenderState::Installing(summary) => (
                self.i18n.t("settings_view.help.installing"),
                summary.clone(),
                None,
                ToastVariant::Default,
            ),
            NativeUpdateRenderState::InstallFinished { status, message } => {
                let (title_key, variant) = match status {
                    oxideterm_update::NativeInstallStatus::ManualActionRequired => (
                        "settings_view.help.update_downloaded",
                        ToastVariant::Warning,
                    ),
                    oxideterm_update::NativeInstallStatus::InstallerLaunched => (
                        "settings_view.help.installer_launched",
                        ToastVariant::Success,
                    ),
                    oxideterm_update::NativeInstallStatus::ReplacementScheduled => (
                        "settings_view.help.replacement_scheduled",
                        ToastVariant::Success,
                    ),
                };
                let status_text = if self.portable
                    && *status == oxideterm_update::NativeInstallStatus::ReplacementScheduled
                {
                    None
                } else {
                    Some(message.clone())
                };
                (self.i18n.t(title_key), status_text, None, variant)
            }
            NativeUpdateRenderState::Error(error) => (
                self.i18n.t("settings_view.help.update_error"),
                (!error.is_empty()).then(|| error.clone()),
                None,
                ToastVariant::Error,
            ),
            NativeUpdateRenderState::Idle
            | NativeUpdateRenderState::Checking
            | NativeUpdateRenderState::UpToDate
            | NativeUpdateRenderState::ManagedByNix => return None,
        };

        let actions = self.render_native_update_notification_actions();
        let close = self.action(NativeUpdateNotificationAction::Dismiss);

        Some(ToastView {
            id: "native-update".to_string(),
            phase: self.phase,
            title,
            description: self.description.clone(),
            status_text,
            progress,
            variant,
            actions,
            close: Some(
                toast_close(&self.tokens)
                    .on_mouse_down(MouseButton::Left, close)
                    .into_any_element(),
            ),
        })
    }

    fn render_native_update_notification_actions(&self) -> Option<AnyElement> {
        let mut actions = Vec::new();

        if self.has_release_notes {
            actions.push(self.native_update_notification_action(
                NativeUpdateNotificationAction::ReleaseNotes,
                self.i18n.t("settings_view.help.release_notes"),
                false,
            ));
        }

        let primary_action = match &self.state {
            NativeUpdateRenderState::Available { .. } => Some((
                NativeUpdateNotificationAction::Download,
                "settings_view.help.download_update",
            )),
            NativeUpdateRenderState::Downloading(_) | NativeUpdateRenderState::Verifying(_) => {
                Some((
                    NativeUpdateNotificationAction::Cancel,
                    "settings_view.help.cancel",
                ))
            }
            NativeUpdateRenderState::Downloaded => Some((
                NativeUpdateNotificationAction::Install,
                "settings_view.help.install_update",
            )),
            NativeUpdateRenderState::Error(_) => Some((
                NativeUpdateNotificationAction::Retry,
                "settings_view.help.retry",
            )),
            _ => None,
        };
        if let Some((action, label_key)) = primary_action {
            actions.push(self.native_update_notification_action(
                action,
                self.i18n.t(label_key),
                true,
            ));
        }

        (!actions.is_empty()).then(|| {
            div()
                .flex()
                .flex_wrap()
                .gap(px(self.tokens.spacing.two))
                .children(actions)
                .into_any_element()
        })
    }

    fn native_update_notification_action(
        &self,
        action: NativeUpdateNotificationAction,
        label: String,
        primary: bool,
    ) -> AnyElement {
        toast_action(&self.tokens, label)
            .cursor_pointer()
            .when(primary, |button| {
                button
                    .border_color(rgb(self.tokens.ui.accent))
                    .bg(rgb(self.tokens.ui.accent))
                    .text_color(rgb(self.tokens.ui.accent_text))
            })
            .when(!primary, |button| {
                button.hover(|button| button.bg(rgb(self.tokens.ui.bg_hover)))
            })
            .on_mouse_down(MouseButton::Left, self.action(action))
            .into_any_element()
    }
}

pub fn release_notes_dialog(
    tokens: &ThemeTokens,
    i18n: &I18n,
    phase: ExitPhase,
    focus: Option<ConfirmDialogAction>,
    description: Option<String>,
    body: AnyElement,
    close: impl Fn(&MouseDownEvent, &mut Window, &mut App) + Clone + 'static,
) -> AnyElement {
    let backdrop = dismissible_dialog_backdrop().on_mouse_down(MouseButton::Left, close.clone());
    let mut header = dialog_header(tokens).child(dialog_title(
        tokens,
        i18n.t("settings_view.help.release_notes"),
    ));
    if let Some(description) = description {
        header = header.child(dialog_description(tokens, description));
    }
    let form = overlay_content_boundary(
        dialog_content(tokens)
            .flex()
            .flex_col()
            .w(px(NATIVE_UPDATE_RELEASE_NOTES_WIDTH))
            .max_w(relative(0.92))
            .h(px(NATIVE_UPDATE_RELEASE_NOTES_HEIGHT))
            .max_h(relative(0.90))
            .child(header)
            .child(
                div()
                    .flex_1()
                    .min_h(px(0.0))
                    .p(px(16.0))
                    .bg(rgb(tokens.ui.bg))
                    .text_color(rgb(tokens.ui.text))
                    .child(body),
            )
            .child(
                dialog_footer(tokens).child(
                    toolbar_button(
                        tokens,
                        i18n.t("settings_view.help.legal_notice_close"),
                        None,
                        ToolbarButtonOptions {
                            button: ButtonOptions {
                                variant: ButtonVariant::Secondary,
                                size: ButtonSize::Sm,
                                radius: ButtonRadius::Md,
                                disabled: false,
                            },
                            focus_visible: focus == Some(ConfirmDialogAction::Cancel),
                            ..Default::default()
                        },
                    )
                    .on_mouse_down(MouseButton::Left, close),
                ),
            ),
    );
    settings_dialog_transition(
        tokens,
        "native-update-release-notes-form",
        backdrop,
        form,
        phase,
    )
}
