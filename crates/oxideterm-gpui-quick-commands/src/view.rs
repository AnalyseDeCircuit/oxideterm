use std::{
    collections::hash_map::DefaultHasher,
    fs,
    hash::{Hash, Hasher},
    path::PathBuf,
};

use gpui::{
    AnyElement, App, Context, Focusable, MouseButton, PathPromptOptions, SharedString, div,
    prelude::*, px, rgb, rgba,
};
use oxideterm_gpui_editor::{EditorContextMenuLabels, EditorSettings, TextEditorView};
use oxideterm_gpui_ui::{
    ConfirmDialogVariant, ConfirmDialogView, StatusPillOptions, StatusTone,
    confirm::confirm_dialog,
    modal::{
        TauriBackdropRole, overlay_content_boundary, staged_dialog_overlay, staged_overlay_root,
    },
    scroll::ScrollableElement,
    status_pill,
    text_input::{TextInputView, text_input_with_viewport},
};
use oxideterm_i18n::I18n;
use oxideterm_quick_commands::{
    FALLBACK_QUICK_COMMAND_CATEGORY_ID, QuickCommandRisk, QuickCommandTargetContext,
    QuickCommandTemplateError, classify_command_risk, quick_command_category_draft_can_save,
    quick_command_has_runtime_substitutions,
};
use zeroize::Zeroizing;

use crate::{bridge::*, state::*};
use oxideterm_gpui_ui::{
    icons::LucideIcon,
    virtual_list::{TauriVirtualListSpec, tauri_virtual_list},
};

mod editor;
mod manager;
mod rows;

fn quick_command_lucide_icon(icon: QuickCommandIcon) -> LucideIcon {
    match icon {
        QuickCommandIcon::Server => LucideIcon::Server,
        QuickCommandIcon::Folder => LucideIcon::Folder,
        QuickCommandIcon::Docker => LucideIcon::Server,
        QuickCommandIcon::Zap => LucideIcon::Zap,
        QuickCommandIcon::Terminal => LucideIcon::Monitor,
    }
}

const QUICK_COMMANDS_MANAGER_WIDTH: f32 = 1120.0;
const QUICK_COMMANDS_MANAGER_HEIGHT: f32 = 720.0;
const QUICK_COMMANDS_MANAGER_COMMAND_LIST_WIDTH: f32 = 360.0;
const QUICK_COMMAND_CATEGORY_PICKER_MAX_HEIGHT: f32 = 72.0;

#[derive(Clone)]
struct QuickCommandCategoryDrag {
    id: String,
    name: String,
    position: gpui::Point<gpui::Pixels>,
    tokens: oxideterm_theme::ThemeTokens,
}

impl gpui::Render for QuickCommandCategoryDrag {
    fn render(&mut self, _: &mut gpui::Window, _: &mut Context<Self>) -> impl IntoElement {
        let tokens = self.tokens;
        div().pl(self.position.x).pt(self.position.y).child(
            div()
                .px_3()
                .py_2()
                .max_w(px(220.0))
                .truncate()
                .rounded(px(tokens.radii.md))
                .border_1()
                .border_color(rgb(tokens.ui.accent))
                .bg(rgb(tokens.ui.bg_panel))
                .text_color(rgb(tokens.ui.text))
                .font_family(tokens.metrics.font_family)
                .text_size(px(tokens.metrics.ui_text_sm))
                .shadow(oxideterm_gpui_ui::theme_overlay_shadow(&tokens))
                .child(self.name.clone()),
        )
    }
}

fn quick_command_icon_label_key(icon: QuickCommandIcon) -> String {
    format!(
        "terminal.quick_commands.icon_{}",
        quick_command_icon_source_id(icon)
    )
}

fn quick_command_editor_frame(
    form: AnyElement,
    footer: AnyElement,
    scroll: &gpui::ScrollHandle,
    tokens: &oxideterm_theme::ThemeTokens,
) -> gpui::Stateful<gpui::Div> {
    div()
        .id("quick-command-editor")
        .w_full()
        .h_full()
        .min_h_0()
        .min_w_0()
        .overflow_hidden()
        .flex()
        .flex_col()
        .bg(rgba((tokens.ui.bg << 8) | 0x59))
        .child(
            div()
                .relative()
                .w_full()
                .flex_1()
                .min_h_0()
                .min_w_0()
                .overflow_hidden()
                .child(
                    div()
                        .id("quick-command-editor-scroll")
                        .size_full()
                        .min_w_0()
                        .overflow_x_hidden()
                        .overflow_y_scroll()
                        .track_scroll(scroll)
                        .child(form),
                )
                .vertical_scrollbar(scroll),
        )
        .child(
            div()
                .id("quick-command-editor-footer")
                .flex_none()
                .border_t_1()
                .border_color(rgb(tokens.ui.border))
                .child(footer),
        )
}

fn quick_commands_export_directory() -> PathBuf {
    std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
}

fn quick_command_risk_tone(risk: QuickCommandRisk) -> StatusTone {
    // Risk colors are presentation policy and remain owned by the GPUI layer.
    match risk {
        QuickCommandRisk::High => StatusTone::Error,
        QuickCommandRisk::Medium => StatusTone::Warning,
    }
}

fn quick_command_risk_label(i18n: &I18n, risk: QuickCommandRisk) -> String {
    match risk {
        QuickCommandRisk::High => i18n.t("terminal.quick_commands.risk_high"),
        QuickCommandRisk::Medium => i18n.t("terminal.quick_commands.risk_medium"),
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum QuickCommandRiskBadge {
    Classified(QuickCommandRisk),
    Dynamic,
}

fn quick_command_risk_badge(template: &str) -> Option<QuickCommandRiskBadge> {
    classify_command_risk(template)
        .map(QuickCommandRiskBadge::Classified)
        .or_else(|| {
            quick_command_has_runtime_substitutions(template)
                .then_some(QuickCommandRiskBadge::Dynamic)
        })
}

fn quick_command_risk_badge_label(i18n: &I18n, badge: QuickCommandRiskBadge) -> String {
    match badge {
        QuickCommandRiskBadge::Classified(risk) => quick_command_risk_label(i18n, risk),
        QuickCommandRiskBadge::Dynamic => i18n.t("terminal.quick_commands.risk_dynamic"),
    }
}

fn quick_command_risk_badge_tone(badge: QuickCommandRiskBadge) -> StatusTone {
    match badge {
        QuickCommandRiskBadge::Classified(risk) => quick_command_risk_tone(risk),
        QuickCommandRiskBadge::Dynamic => StatusTone::Warning,
    }
}

fn quick_command_template_error_label(i18n: &I18n, error: &QuickCommandTemplateError) -> String {
    let (key, replacements) = match error {
        QuickCommandTemplateError::UnterminatedToken => (
            "terminal.quick_commands.error_unterminated_token",
            Vec::new(),
        ),
        QuickCommandTemplateError::UnknownToken(token) => (
            "terminal.quick_commands.error_unknown_token",
            vec![("{{token}}", token.as_str())],
        ),
        QuickCommandTemplateError::UnknownModifier(modifier) => (
            "terminal.quick_commands.error_unknown_modifier",
            vec![("{{modifier}}", modifier.as_str())],
        ),
        QuickCommandTemplateError::UnknownParameter(parameter) => (
            "terminal.quick_commands.error_unknown_parameter",
            vec![("{{parameter}}", parameter.as_str())],
        ),
        QuickCommandTemplateError::TooManyParameterValues => (
            "terminal.quick_commands.error_too_many_parameter_values",
            Vec::new(),
        ),
        QuickCommandTemplateError::ParameterValueTooLong(parameter) => (
            "terminal.quick_commands.error_parameter_value_too_long",
            vec![("{{parameter}}", parameter.as_str())],
        ),
        QuickCommandTemplateError::ExpandedCommandTooLong { target } => (
            "terminal.quick_commands.error_expanded_command_too_long",
            vec![("{{target}}", target.as_str())],
        ),
        QuickCommandTemplateError::MissingParameter(parameter) => (
            "terminal.quick_commands.error_missing_parameter",
            vec![("{{parameter}}", parameter.as_str())],
        ),
        QuickCommandTemplateError::InvalidChoice { parameter } => (
            "terminal.quick_commands.error_invalid_choice",
            vec![("{{parameter}}", parameter.as_str())],
        ),
        QuickCommandTemplateError::MissingContext { target, field } => (
            "terminal.quick_commands.error_missing_context",
            vec![
                ("{{target}}", target.as_str()),
                ("{{field}}", field.as_str()),
            ],
        ),
    };
    replacements
        .into_iter()
        .fold(i18n.t(key), |label, (placeholder, value)| {
            label.replace(placeholder, value)
        })
}

pub struct QuickCommandsView {
    pub(crate) state: QuickCommandsAccess,
    pub(crate) bindings: QuickCommandsBindings,
    frame: QuickCommandsFrame,
    manager: bool,
}

impl std::ops::Deref for QuickCommandsView {
    type Target = QuickCommandsFrame;

    fn deref(&self) -> &Self::Target {
        &self.frame
    }
}

impl gpui::Render for QuickCommandsView {
    fn render(&mut self, _: &mut gpui::Window, cx: &mut Context<Self>) -> impl IntoElement {
        let Some(mut frame) = (self.bindings.frame)(cx) else {
            return div().into_any_element();
        };
        // Runtime context may include selected terminal text and must not outlive this render.
        let target = frame.target.take();
        let execution_targets = std::mem::take(&mut frame.execution_targets);
        self.frame = frame;
        if self.manager {
            self.render_quick_commands_manager_modal(target.as_ref(), cx)
        } else {
            self.render_terminal_quick_commands_panel(target.as_ref(), &execution_targets, cx)
        }
    }
}

pub struct QuickCommandsViews {
    pub dock: gpui::Entity<QuickCommandsView>,
    pub manager: gpui::Entity<QuickCommandsView>,
}

impl QuickCommandsViews {
    pub fn new(
        state: QuickCommandsAccess,
        bindings: QuickCommandsBindings,
        frame: QuickCommandsFrame,
        cx: &mut App,
    ) -> Self {
        let dock = cx.new(|_| QuickCommandsView {
            state: state.clone(),
            bindings: bindings.clone(),
            frame: frame.clone(),
            manager: false,
        });
        let manager = cx.new(|_| QuickCommandsView {
            state,
            bindings,
            frame,
            manager: true,
        });
        Self { dock, manager }
    }
}

#[derive(Clone, Copy)]
enum SelectableTextRole {
    PlainDocument,
    NonSelectable,
}

fn selection_id(scope: &str, key: impl Hash) -> u64 {
    let mut hasher = DefaultHasher::new();
    scope.hash(&mut hasher);
    key.hash(&mut hasher);
    hasher.finish()
}

impl QuickCommandsView {
    pub(crate) fn dispatch(
        &self,
        request: QuickCommandsRequest,
        window: Option<&mut gpui::Window>,
        cx: &mut App,
    ) {
        (self.bindings.dispatch)(request, window, cx);
    }

    pub(crate) fn quick_commands_render_snapshot(
        &self,
        target: Option<&QuickCommandTargetContext>,
        cx: &App,
    ) -> QuickCommandsRenderSnapshot {
        self.state.read(cx).render_snapshot(target)
    }

    fn close_quick_commands_manager(&mut self, cx: &mut Context<Self>) {
        if self.state.update(cx, |state, _| state.close_manager()) {
            self.dispatch(QuickCommandsRequest::ClearIme, None, cx);
            self.dispatch(QuickCommandsRequest::WindowLayoutChanged, None, cx);
            cx.notify();
        }
    }

    fn close_terminal_quick_commands_panel(&mut self, cx: &mut Context<Self>) {
        if self.state.update(cx, |state, _| state.close()) {
            self.dispatch(QuickCommandsRequest::ClearIme, None, cx);
            self.dispatch(QuickCommandsRequest::WindowLayoutChanged, None, cx);
            cx.notify();
        }
    }

    fn cancel_quick_command_category_delete(&mut self, cx: &mut Context<Self>) {
        if self
            .state
            .update(cx, |state, _| state.cancel_category_delete())
        {
            cx.notify();
        }
    }

    fn confirm_quick_command_category_delete(&mut self, cx: &mut Context<Self>) {
        if self
            .state
            .update(cx, |state, _| state.confirm_category_delete())
        {
            self.dispatch(QuickCommandsRequest::CheckQuickBarCategory, None, cx);
            cx.notify();
        }
    }

    fn cancel_terminal_quick_command_execution(&mut self, cx: &mut Context<Self>) {
        if self.state.update(cx, |state, _| state.cancel_execution()) {
            cx.notify();
        }
    }

    fn insert_quick_command_into_command_bar(
        &mut self,
        command: String,
        keep_open: bool,
        window: &mut gpui::Window,
        cx: &mut Context<Self>,
    ) {
        let command = self
            .state
            .update(cx, |state, _| state.prepare_insertion(command, keep_open));
        self.dispatch(QuickCommandsRequest::Insert(command), Some(window), cx);
    }

    fn push_workspace_notice(&self, notice: QuickCommandsNotice, cx: &mut App) {
        self.dispatch(QuickCommandsRequest::Notice(notice), None, cx);
    }

    pub(crate) fn render_lucide_icon(icon: LucideIcon, size: f32, color: gpui::Rgba) -> AnyElement {
        gpui::svg()
            .path(icon.path())
            .size(px(size))
            .text_color(color)
            .into_any_element()
    }

    pub(crate) fn workspace_icon_action_button(
        &self,
        icon: LucideIcon,
        size: f32,
        color: gpui::Rgba,
        options: oxideterm_gpui_ui::button::IconButtonOptions,
        listener: impl Fn(&mut Self, &gpui::MouseDownEvent, &mut gpui::Window, &mut Context<Self>)
        + 'static,
        cx: &mut Context<Self>,
    ) -> gpui::Div {
        let actionable = !(options.disabled || options.loading);
        oxideterm_gpui_ui::button::icon_button(
            &self.tokens,
            Self::render_lucide_icon(icon, size, color),
            options,
        )
        .when(actionable, |button| {
            button.on_mouse_down(MouseButton::Left, cx.listener(listener))
        })
        .when(!actionable, |button| {
            button.on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
        })
    }

    fn render_display_text_with_role(
        &self,
        role: SelectableTextRole,
        scope: &str,
        key: impl Hash,
        text: impl Into<String>,
        color: u32,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let text = text.into();
        match role {
            SelectableTextRole::NonSelectable => div()
                .min_w_0()
                .font_family(self.ui_font.clone())
                .text_color(rgb(color))
                .child(text)
                .into_any_element(),
            SelectableTextRole::PlainDocument => (self.bindings.selectable)(
                QuickCommandsSelectableText {
                    id: selection_id(scope, (key, text.as_str())),
                    text,
                    color,
                    row: None,
                },
                cx,
            ),
        }
    }

    fn render_row_safe_selectable_display_text_in_group(
        &self,
        group: u64,
        scope: &str,
        key: impl Hash,
        order: usize,
        text: impl Into<String>,
        color: u32,
        font: Option<SharedString>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        self.render_row_safe_selectable_display_text_in_group_with_alpha(
            group, scope, key, order, text, color, 1.0, font, cx,
        )
    }

    fn render_row_safe_selectable_display_text_in_group_with_alpha(
        &self,
        group: u64,
        scope: &str,
        key: impl Hash,
        order: usize,
        text: impl Into<String>,
        color: u32,
        alpha: f32,
        font: Option<SharedString>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let text = text.into();
        (self.bindings.selectable)(
            QuickCommandsSelectableText {
                id: selection_id(scope, (key, text.as_str())),
                text,
                color,
                row: Some((group, order, alpha, font)),
            },
            cx,
        )
    }
}

#[cfg(test)]
mod terminal_command_bar_quick_command_tests {
    use super::{
        QuickCommandConfirmationPolicy, QuickCommandEditorDraft, QuickCommandMatchField,
        QuickCommandParameterEditorDraft, QuickCommandParameterKind, QuickCommandRiskBadge,
        quick_command_editor_can_save, quick_command_risk_badge,
    };
    use gpui::{
        Context, InteractiveElement, IntoElement, ParentElement, Render, ScrollHandle, Styled, div,
        px,
    };

    #[test]
    fn quick_bar_filters_independently_and_keeps_empty_groups_selectable() {
        let directory = tempfile::tempdir().unwrap();
        let mut state =
            super::TerminalQuickCommandsState::load(&directory.path().join("settings.json"));
        let template = oxideterm_quick_commands::default_quick_commands().remove(0);
        state.store.commands = [("one", "system"), ("two", "files"), ("ssh-only", "system")]
            .into_iter()
            .map(|(id, category)| {
                let mut command = template.clone();
                command.id = id.into();
                command.category = category.into();
                command.availability = Default::default();
                if id == "ssh-only" {
                    command.availability.protocols = vec![super::QuickCommandTargetProtocol::Ssh];
                }
                command
            })
            .collect();
        state.store.active_category = "files".into();
        state.store.query = "no matches".into();
        let target = oxideterm_quick_commands::QuickCommandTargetContext {
            target_id: "ssh".into(),
            label: "SSH".into(),
            protocol: super::QuickCommandTargetProtocol::Ssh,
            values: Default::default(),
        };
        for (category, target, expected) in [
            (None, None, vec!["one", "two"]),
            (Some("system"), None, vec!["one"]),
            (Some("system"), Some(&target), vec!["one", "ssh-only"]),
            (Some("custom"), None, vec![]),
            (Some("deleted"), None, vec!["one", "two"]),
        ] {
            let (categories, commands) = state.quick_bar_snapshot(target, category);
            assert_eq!(
                commands.iter().map(|c| c.id.as_str()).collect::<Vec<_>>(),
                expected,
                "{category:?}"
            );
            assert!(categories.iter().any(|c| c.id == "custom"));
        }
    }

    #[test]
    fn quick_bar_group_preference_survives_reload_and_clearing() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("settings.json");
        let mut store = oxideterm_settings::SettingsStore::load_from_path(&path).unwrap();
        store.settings_mut().terminal.command_bar.quick_bar_category = Some("files".into());
        store.save().unwrap();
        let mut loaded = oxideterm_settings::SettingsStore::load_from_path(&path).unwrap();
        assert_eq!(
            loaded
                .settings()
                .terminal
                .command_bar
                .quick_bar_category
                .as_deref(),
            Some("files")
        );
        loaded
            .settings_mut()
            .terminal
            .command_bar
            .quick_bar_category = None;
        loaded.save().unwrap();
        assert_eq!(
            oxideterm_settings::SettingsStore::load_from_path(&path)
                .unwrap()
                .settings()
                .terminal
                .command_bar
                .quick_bar_category,
            None
        );
    }

    #[test]
    fn category_color_editor_validates_persists_and_resets_to_theme() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("settings.json");
        let mut state = super::TerminalQuickCommandsState::load(&path);
        let category = state
            .store
            .categories
            .iter()
            .find(|c| c.id == "system")
            .unwrap()
            .clone();
        state.start_category_edit(category);
        state.set_focused_input(super::QuickCommandInput::CategoryColor);
        state.replace_input(super::QuickCommandInput::CategoryColor, None, "#GG0000");
        assert!(!state.save_category_editor());
        assert_eq!(
            state.input_value(super::QuickCommandInput::CategoryColor),
            Some("#GG0000")
        );
        state.replace_input(
            super::QuickCommandInput::CategoryColor,
            Some(0..7),
            "#22D3EE",
        );
        assert!(state.save_category_editor());
        let loaded = oxideterm_quick_commands::load_snapshot(&path).unwrap();
        let category = loaded
            .categories
            .into_iter()
            .find(|c| c.id == "system")
            .unwrap();
        assert_eq!(category.icon_color.as_deref(), Some("#22d3ee"));
        state.start_category_edit(category);
        state.set_focused_input(super::QuickCommandInput::CategoryColor);
        state.replace_input(super::QuickCommandInput::CategoryColor, Some(0..7), "");
        assert!(state.save_category_editor());
        let loaded = oxideterm_quick_commands::load_snapshot(&path).unwrap();
        assert_eq!(
            loaded
                .categories
                .iter()
                .find(|c| c.id == "system")
                .unwrap()
                .icon_color,
            None
        );
    }

    struct EditorFormFixture {
        scroll: ScrollHandle,
        rows: usize,
    }

    impl Render for EditorFormFixture {
        fn render(&mut self, _: &mut gpui::Window, _: &mut Context<Self>) -> impl IntoElement {
            let rows = self.rows;
            let form = div()
                .w_full()
                .flex()
                .flex_col()
                .flex_none()
                .children((0..self.rows).map(|index| {
                    div()
                        .h(px(64.0))
                        .flex_none()
                        .debug_selector(move || {
                            if index + 1 == rows {
                                "last-field".into()
                            } else {
                                format!("field-{index}")
                            }
                        })
                        .child(format!("Parameter {index}"))
                }));
            super::quick_command_editor_frame(
                form.into_any_element(),
                div()
                    .h(px(48.0))
                    .debug_selector(|| "footer".into())
                    .into_any_element(),
                &self.scroll,
                &oxideterm_theme::default_tokens(),
            )
        }
    }

    #[gpui::test]
    fn long_command_form_reaches_last_field_without_moving_footer(cx: &mut gpui::TestAppContext) {
        let (view, cx) = cx.add_window_view(|_, _| EditorFormFixture {
            scroll: ScrollHandle::new(),
            rows: 20,
        });
        for height in [300.0, 180.0] {
            cx.simulate_resize(gpui::size(px(480.0), px(height)));
            cx.update(|window, app| {
                window.draw(app).clear(app);
            });
            let footer = cx.debug_bounds("footer").unwrap();
            assert_eq!(footer.bottom(), px(height));
            let scroll = view.read_with(cx, |view, _| view.scroll.clone());
            let max_offset = scroll.max_offset();
            assert!(max_offset.y > px(64.0 * 15.0));
            scroll.set_offset(gpui::point(px(0.0), -max_offset.y));
            for _ in 0..3 {
                cx.update(|window, app| {
                    window.refresh();
                    window.draw(app).clear(app);
                });
                assert_eq!(scroll.max_offset(), max_offset);
                assert_eq!(cx.debug_bounds("footer").unwrap(), footer);
                let last = cx.debug_bounds("last-field").unwrap();
                assert!(last.top() >= scroll.bounds().top());
                assert!((last.bottom() - scroll.bounds().bottom()).abs() < px(1.0));
            }
        }
    }

    #[test]
    fn dock_pinning_keeps_launcher_open_and_blur_preserves_confirmation_until_close() {
        let directory = tempfile::tempdir().unwrap();
        let mut commands =
            super::TerminalQuickCommandsState::load(&directory.path().join("settings.json"));
        let command = oxideterm_quick_commands::default_quick_commands().remove(0);
        for pinned in [false, true] {
            commands.open = true;
            commands.pinned = pinned;
            commands.set_focused_input(super::QuickCommandInput::Search);
            assert_eq!(
                commands.prepare_insertion("printf 'hello'".into(), pinned),
                "printf 'hello'"
            );
            assert_eq!(commands.is_open(), pinned);
            assert_eq!(commands.focused_input(), None);
            commands.request_execution(command.clone());
            commands.set_focused_input(super::QuickCommandInput::Search);
            commands.blur_input();
            assert!(commands.is_open());
            assert_eq!(commands.focused_input(), None);
            assert_eq!(
                commands.pending_execution.as_ref().unwrap().command.id,
                command.id
            );
            commands.set_focused_input(super::QuickCommandInput::ParameterDefault(0));
            commands.finish_execution();
            assert_eq!(commands.is_open(), pinned);
            assert_eq!(commands.focused_input(), None);
            assert!(commands.pending_execution.is_none());
            commands.request_execution(command.clone());
            commands.close();
            assert!(!commands.is_open());
            assert!(commands.pending_execution.is_none());
        }
    }

    #[test]
    fn quick_command_editor_validates_templates_and_secret_defaults() {
        let mut draft = QuickCommandEditorDraft {
            id: None,
            name: "Deploy".to_string(),
            command: "deploy {{param.sevrice}}".to_string(),
            category: "custom".to_string(),
            description: String::new(),
            host_patterns: String::new(),
            match_field: QuickCommandMatchField::Host,
            parameters: vec![QuickCommandParameterEditorDraft {
                name: "service".to_string(),
                label: "Service".to_string(),
                kind: QuickCommandParameterKind::Text,
                default_value: String::new(),
                choices: String::new(),
                required: true,
            }],
            protocols: Vec::new(),
            confirmation: QuickCommandConfirmationPolicy::Inherit,
            created_at: 1,
            sort_order: 0,
        };

        assert!(!quick_command_editor_can_save(&draft));
        draft.command = "deploy {{param.service|sh}}".to_string();
        assert!(quick_command_editor_can_save(&draft));
        draft.parameters[0].kind = QuickCommandParameterKind::Secret;
        draft.parameters[0].default_value = "must-not-persist".to_string();
        assert!(!quick_command_editor_can_save(&draft));
        draft.parameters[0].default_value.clear();
        assert!(quick_command_editor_can_save(&draft));
    }

    #[test]
    fn parameterized_commands_show_dynamic_risk_until_expansion() {
        assert_eq!(
            quick_command_risk_badge("echo {{param.value}}"),
            Some(QuickCommandRiskBadge::Dynamic)
        );
    }
}
