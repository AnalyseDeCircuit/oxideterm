use std::rc::Rc;

use gpui::{
    AnyElement, App, MouseButton, MouseDownEvent, Window, div, prelude::*, px, relative, rgb,
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
    motion::ExitPhase,
    toast::{ToastVariant, ToastView, toast_action, toast_close},
};
use oxideterm_i18n::I18n;
use oxideterm_theme::ThemeTokens;

use crate::{
    dialog::settings_dialog_transition,
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
