use gpui::{AnyElement, App, MouseButton, div, prelude::*, px, relative, rgb};
use oxideterm_gpui_markdown::{
    MarkdownCodeBlockActions, MarkdownOptions, MarkdownVirtualListScrollHandle,
    markdown_virtual_with_code_actions,
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
};
use oxideterm_i18n::I18n;
use oxideterm_theme::ThemeTokens;

use crate::{
    dialog::settings_dialog_transition, entity::SettingsWorkspaceEntity,
    update_view::release_notes_dialog,
};

const HELP_LEGAL_MARKDOWN: &str =
    include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/../../LEGAL.md"));
const HELP_THIRD_PARTY_MARKDOWN: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../THIRD_PARTY_NOTICES.md"
));

pub struct SettingsMarkdownDialog<'a> {
    pub tokens: &'a ThemeTokens,
    pub i18n: &'a I18n,
    pub phase: ExitPhase,
    pub focus: Option<ConfirmDialogAction>,
    pub options: MarkdownOptions,
    pub actions: MarkdownCodeBlockActions,
}

impl SettingsMarkdownDialog<'_> {
    pub fn release_notes(
        mut self,
        settings: &SettingsWorkspaceEntity,
        scroll: &MarkdownVirtualListScrollHandle,
        finish: impl Fn(&mut App) + Clone + 'static,
    ) -> AnyElement {
        self.options.base_font_size = self.tokens.metrics.ui_text_sm;
        self.options.block_gap = 8.0;
        let (body, description) = settings
            .native_update_release_notes()
            .map(|notes| (notes.body, notes.description))
            .unwrap_or_else(|| (self.i18n.t("settings_view.help.no_changelog"), None));
        let body = markdown_virtual_with_code_actions(
            "native-update-release-notes-markdown",
            self.tokens,
            &body,
            &self.options,
            scroll,
            &self.actions,
        )
        .into_any_element();
        release_notes_dialog(
            self.tokens,
            self.i18n,
            self.phase,
            self.focus,
            description,
            body,
            move |_, _, cx| {
                finish(cx);
                cx.stop_propagation();
            },
        )
    }

    pub fn legal_notice(
        mut self,
        third_party: bool,
        scroll: &MarkdownVirtualListScrollHandle,
        finish: impl Fn(bool, &mut App) + Clone + 'static,
    ) -> AnyElement {
        self.options.base_font_size = self.tokens.metrics.ui_text_sm;
        self.options.block_gap = 8.0;
        let options = self.options;
        let backdrop = dismissible_dialog_backdrop().on_mouse_down(MouseButton::Left, {
            let finish = finish.clone();
            move |_, _, cx| {
                finish(false, cx);
                cx.stop_propagation();
            }
        });
        let button = toolbar_button(
            self.tokens,
            self.i18n.t("settings_view.help.legal_notice_close"),
            None,
            ToolbarButtonOptions {
                button: ButtonOptions {
                    variant: ButtonVariant::Secondary,
                    radius: ButtonRadius::Md,
                    size: ButtonSize::Sm,
                    disabled: false,
                },
                focus_visible: self.focus == Some(ConfirmDialogAction::Cancel),
                ..Default::default()
            },
        )
        .on_mouse_down(MouseButton::Left, move |_, _, cx| {
            finish(true, cx);
            cx.stop_propagation();
        });
        let form = overlay_content_boundary(
            dialog_content(self.tokens)
                .flex()
                .flex_col()
                .w(px(760.0))
                .max_w(relative(0.92))
                .h(px(720.0))
                .max_h(relative(0.90))
                .child(
                    dialog_header(self.tokens)
                        .child(dialog_title(
                            self.tokens,
                            self.i18n.t(if third_party {
                                "settings_view.help.third_party_notices"
                            } else {
                                "settings_view.help.disclaimer"
                            }),
                        ))
                        .child(dialog_description(
                            self.tokens,
                            self.i18n.t(if third_party {
                                "settings_view.help.third_party_notices_description"
                            } else {
                                "settings_view.help.legal_notice_description"
                            }),
                        )),
                )
                .child(
                    div()
                        .flex_1()
                        .min_h(px(0.0))
                        .p(px(16.0))
                        .bg(rgb(self.tokens.ui.bg))
                        .text_color(rgb(self.tokens.ui.text))
                        .child(markdown_virtual_with_code_actions(
                            "settings-help-legal-notice-markdown",
                            self.tokens,
                            if third_party {
                                HELP_THIRD_PARTY_MARKDOWN
                            } else {
                                HELP_LEGAL_MARKDOWN
                            },
                            &options,
                            scroll,
                            &self.actions,
                        )),
                )
                .child(dialog_footer(self.tokens).child(button)),
        );
        settings_dialog_transition(
            self.tokens,
            "help-legal-notice-form",
            backdrop,
            form,
            self.phase,
        )
    }
}
