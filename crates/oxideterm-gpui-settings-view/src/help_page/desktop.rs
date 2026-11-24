use super::*;
use crate::{
    components::SettingsCalloutTone,
    entity::SettingsWorkspaceEntity,
    native_update::{
        NativeUpdateRenderState, native_update_progress_hint, native_update_progress_ratio,
    },
};
use gpui::{Entity, relative};

pub struct HelpPage<'a> {
    pub controls: SettingsControls<'a>,
    pub state: &'a Entity<SettingsWorkspaceEntity>,
    pub year: &'a str,
    pub dispatch: Rc<dyn Fn(HelpRequest, &mut App)>,
}

impl<'a> Deref for HelpPage<'a> {
    type Target = SettingsControls<'a>;

    fn deref(&self) -> &Self::Target {
        &self.controls
    }
}

impl HelpPage<'_> {
    pub fn render(&self, section: SettingsSectionId, cx: &App) -> AnyElement {
        let page = HelpPresentation {
            controls: self.controls.clone(),
            year: self.year,
            dispatch: self.dispatch.clone(),
        };
        match section {
            SettingsSectionId::HelpVersion => {
                let portable_install = self.resolved_help_portable_mode(cx);
                let footer = self.help_update_footer(&page, portable_install, cx);
                page.help_version_card(portable_install, Some(footer), cx)
            }
            SettingsSectionId::HelpDiagnostics => page.help_diagnostics_card(true, cx),
            _ => page.section(section, cx),
        }
    }

    fn help_update_footer(
        &self,
        page: &HelpPresentation<'_>,
        is_portable: bool,
        cx: &App,
    ) -> AnyElement {
        div()
            .pt(px(16.0))
            .border_t_1()
            .border_color(rgba((self.tokens.ui.border << 8) | 0x66))
            .flex()
            .flex_col()
            .gap(px(12.0))
            .child(if is_portable {
                div()
                    .flex()
                    .flex_col()
                    .gap(px(12.0))
                    .child(self.help_portable_update_notice())
                    .child(self.help_update_status_area(page, cx))
                    .into_any_element()
            } else {
                self.help_update_status_area(page, cx)
            })
            .into_any_element()
    }

    fn help_portable_update_notice(&self) -> AnyElement {
        self.settings_callout(
            SettingsCalloutTone::Warning,
            Some("lucide/shield.svg"),
            div()
                .flex()
                .flex_col()
                .gap(px(4.0))
                .child(
                    div()
                        .text_size(px(self.tokens.metrics.ui_text_sm))
                        .font_weight(gpui::FontWeight::MEDIUM)
                        .text_color(rgb(self.tokens.ui.text))
                        .child(self.i18n.t("settings_view.help.portable_updates")),
                )
                .child(
                    div()
                        .text_size(px(self.tokens.metrics.ui_text_xs))
                        .text_color(rgb(self.tokens.ui.text_muted))
                        .child(self.i18n.t("settings_view.help.portable_updates_hint")),
                )
                .into_any_element(),
        )
    }

    fn help_update_status_area(&self, page: &HelpPresentation<'_>, cx: &App) -> AnyElement {
        let update_state = self.state.read(cx).native_update_render_state();
        let button_icon = if matches!(update_state, NativeUpdateRenderState::Checking) {
            "lucide/loader-circle.svg"
        } else {
            "lucide/refresh-cw.svg"
        };
        let disabled = matches!(
            update_state,
            NativeUpdateRenderState::Checking
                | NativeUpdateRenderState::Downloading(_)
                | NativeUpdateRenderState::Verifying(_)
                | NativeUpdateRenderState::Installing(_)
        );

        let mut area = div().flex().flex_col().gap(px(12.0)).child(
            div()
                .flex()
                .items_center()
                .gap(px(12.0))
                .child(page.help_outline_button_with_disabled(
                    self.i18n.t("settings_view.help.check_update"),
                    button_icon,
                    disabled,
                    HelpRequest::Check,
                    cx,
                ))
                .children(self.help_update_status_inline(&update_state)),
        );

        if let Some(detail) = self.help_update_detail(page, &update_state, cx) {
            area = area.child(detail);
        }

        area.into_any_element()
    }

    fn help_update_status_inline(
        &self,
        update_state: &NativeUpdateRenderState,
    ) -> Option<AnyElement> {
        let (label, icon, color) = match update_state {
            NativeUpdateRenderState::Checking => (
                self.i18n.t("settings_view.help.checking"),
                None,
                self.tokens.ui.text_muted,
            ),
            NativeUpdateRenderState::UpToDate => (
                self.i18n.t("settings_view.help.up_to_date"),
                Some("lucide/check-circle.svg"),
                self.tokens.ui.success,
            ),
            NativeUpdateRenderState::ManagedByNix => (
                self.i18n.t("settings_view.help.managed_by_nix"),
                None,
                self.tokens.ui.text_muted,
            ),
            NativeUpdateRenderState::Verifying(_) => (
                self.i18n.t("settings_view.help.verifying"),
                None,
                self.tokens.ui.text_muted,
            ),
            NativeUpdateRenderState::Installing(summary) => (
                summary
                    .clone()
                    .unwrap_or_else(|| self.i18n.t("settings_view.help.installing")),
                None,
                self.tokens.ui.text_muted,
            ),
            NativeUpdateRenderState::Downloaded => (
                self.i18n.t("settings_view.help.update_downloaded"),
                Some("lucide/check-circle.svg"),
                self.tokens.ui.success,
            ),
            NativeUpdateRenderState::InstallFinished { status, .. } => {
                let label_key = match status {
                    oxideterm_update::NativeInstallStatus::ManualActionRequired => {
                        "settings_view.help.update_downloaded"
                    }
                    oxideterm_update::NativeInstallStatus::InstallerLaunched => {
                        "settings_view.help.installer_launched"
                    }
                    oxideterm_update::NativeInstallStatus::ReplacementScheduled => {
                        "settings_view.help.replacement_scheduled"
                    }
                };
                (
                    self.i18n.t(label_key),
                    Some("lucide/check-circle.svg"),
                    self.tokens.ui.success,
                )
            }
            NativeUpdateRenderState::Error(error) => (
                if error.is_empty() {
                    self.i18n.t("settings_view.help.update_error")
                } else {
                    error.clone()
                },
                Some("lucide/alert-circle.svg"),
                self.tokens.ui.error,
            ),
            NativeUpdateRenderState::Idle
            | NativeUpdateRenderState::Available { .. }
            | NativeUpdateRenderState::Downloading(_) => return None,
        };

        let mut row = div()
            .flex()
            .items_center()
            .gap(px(6.0))
            .text_size(px(self.tokens.metrics.ui_text_sm))
            .text_color(rgb(color));
        if let Some(icon) = icon {
            row = row.child(render_icon(icon, 14.0, rgb(color)));
        }
        Some(row.child(label).into_any_element())
    }

    fn help_update_detail(
        &self,
        page: &HelpPresentation<'_>,
        update_state: &NativeUpdateRenderState,
        cx: &App,
    ) -> Option<AnyElement> {
        match update_state {
            NativeUpdateRenderState::Available {
                version,
                has_release_notes,
            } => {
                let mut actions = div()
                    .flex()
                    .flex_wrap()
                    .justify_end()
                    .gap(px(self.tokens.spacing.two));
                if *has_release_notes {
                    actions = actions.child(page.help_outline_button(
                        self.i18n.t("settings_view.help.release_notes"),
                        "lucide/book-open.svg",
                        HelpRequest::ReleaseNotes,
                        cx,
                    ));
                }
                actions = actions.child(page.help_outline_button(
                    self.i18n.t("settings_view.help.download_update"),
                    "lucide/download.svg",
                    HelpRequest::Download,
                    cx,
                ));

                Some(
                    div()
                        .flex()
                        .flex_col()
                        .gap(px(12.0))
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap(px(8.0))
                                .text_size(px(self.tokens.metrics.ui_text_sm))
                                .text_color(rgb(self.tokens.ui.text))
                                .child(self.i18n.t("settings_view.help.update_available"))
                                .child(
                                    div()
                                        .text_color(rgb(self.tokens.ui.accent))
                                        .font_weight(gpui::FontWeight::MEDIUM)
                                        .child(format!("v{version}")),
                                ),
                        )
                        .child(actions)
                        .into_any_element(),
                )
            }
            NativeUpdateRenderState::Downloading(status) => {
                Some(self.help_transfer_progress(page, status.as_ref(), false, cx))
            }
            NativeUpdateRenderState::Verifying(status) => {
                Some(self.help_transfer_progress(page, status.as_ref(), true, cx))
            }
            NativeUpdateRenderState::Downloaded => Some(
                div()
                    .flex()
                    .justify_end()
                    .child(page.help_outline_button(
                        self.i18n.t("settings_view.help.install_update"),
                        "lucide/download.svg",
                        HelpRequest::Install,
                        cx,
                    ))
                    .into_any_element(),
            ),
            _ => None,
        }
    }

    fn resolved_help_portable_mode(&self, cx: &App) -> bool {
        self.state
            .read(cx)
            .portable_mode()
            .unwrap_or_else(|| oxideterm_portable_runtime::is_portable_mode().unwrap_or(false))
    }

    fn help_transfer_progress(
        &self,
        page: &HelpPresentation<'_>,
        status: Option<&oxideterm_update::ResumableUpdateStatus>,
        verifying: bool,
        cx: &App,
    ) -> AnyElement {
        let progress_ratio = if verifying {
            1.0
        } else {
            status.and_then(native_update_progress_ratio).unwrap_or(0.0)
        };
        let fallback_key = if verifying {
            "settings_view.help.verifying"
        } else {
            "settings_view.help.downloading"
        };

        div()
            .flex()
            .flex_col()
            .gap(px(self.tokens.spacing.two))
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap(px(12.0))
                    .child(
                        div()
                            .min_w(px(0.0))
                            .text_size(px(self.tokens.metrics.ui_text_sm))
                            .text_color(rgb(self.tokens.ui.text_muted))
                            .child(
                                status
                                    .map(native_update_progress_hint)
                                    .unwrap_or_else(|| self.i18n.t(fallback_key)),
                            ),
                    )
                    .child(page.help_outline_button(
                        self.i18n.t("settings_view.help.cancel"),
                        "lucide/x.svg",
                        HelpRequest::Cancel,
                        cx,
                    )),
            )
            .child(
                div()
                    .h(px(self.tokens.spacing.one))
                    .w_full()
                    .overflow_hidden()
                    .rounded_full()
                    .bg(rgba((self.tokens.ui.border << 8) | 0x80))
                    .child(
                        div()
                            .h_full()
                            .rounded_full()
                            .bg(rgb(self.tokens.ui.accent))
                            .w(relative(progress_ratio)),
                    ),
            )
            .into_any_element()
    }
}
