use gpui::{
    AnyElement, App, AppContext, Context, Div, Entity, Focusable, MouseButton, MouseDownEvent,
    Render, Window, div, prelude::*, px,
};
use oxideterm_gpui_editor::TextEditorView;
use oxideterm_gpui_ui::{
    button::{
        ButtonOptions, ButtonRadius, ButtonSize, ButtonVariant, ToolbarButtonOptions,
        toolbar_button,
    },
    modal::{
        dialog_description, dialog_footer, dialog_header, dialog_title,
        dismissible_dialog_backdrop, overlay_content_boundary,
    },
};
use oxideterm_i18n::I18n;
use oxideterm_settings::PersistedSettings;
use oxideterm_theme::ThemeTokens;

use crate::components::{SettingsWindowDispatch, SettingsWindowRequest};

const AI_TEXT_EDITOR_MODAL_WIDTH: f32 = 760.0;
const AI_TEXT_EDITOR_MODAL_HEIGHT: f32 = 640.0;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AiTextEditorDialog {
    SystemPrompt,
    Memory,
}

pub struct AiTextEditorStyle {
    pub tokens: ThemeTokens,
    pub i18n: I18n,
    pub font_family: String,
    pub font_weight: f32,
    pub font_fallback: Option<String>,
    pub font_size: f32,
    pub line_height: f32,
    pub background_active: bool,
}

#[derive(Default)]
pub struct AiTextEditor {
    draft: Option<AiTextEditorDraft>,
}

struct AiTextEditorDraft {
    kind: AiTextEditorDialog,
    editor: Entity<TextEditorView>,
    style: AiTextEditorStyle,
    dispatch: SettingsWindowDispatch,
}

impl Render for AiTextEditor {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.draft
            .as_ref()
            .map(|draft| self.render_modal(draft, cx))
            .unwrap_or_else(|| div().into_any_element())
    }
}

impl AiTextEditor {
    pub fn is_open(&self) -> bool {
        self.draft.is_some()
    }

    pub fn dismiss(&mut self, cx: &mut Context<Self>) {
        self.draft = None;
        cx.notify();
    }

    fn toolbar_button(
        tokens: &ThemeTokens,
        label: String,
        options: ToolbarButtonOptions,
        action: impl Fn(&MouseDownEvent, &mut Window, &mut App) + 'static,
    ) -> Div {
        toolbar_button(tokens, label, None, options).on_mouse_down(MouseButton::Left, action)
    }

    fn render_modal(&self, draft: &AiTextEditorDraft, cx: &mut Context<Self>) -> AnyElement {
        let dialog = draft.kind;
        let tokens = &draft.style.tokens;
        let i18n = &draft.style.i18n;
        let editor = draft.editor.clone();
        let (title_key, description_key) = match dialog {
            AiTextEditorDialog::SystemPrompt => (
                "settings_view.ai.system_prompt_title",
                "settings_view.ai.system_prompt_hint",
            ),
            AiTextEditorDialog::Memory => (
                "settings_view.ai.memory_title",
                "settings_view.ai.memory_hint",
            ),
        };
        let modal = oxideterm_gpui_ui::modal_container(tokens)
            .w(px(AI_TEXT_EDITOR_MODAL_WIDTH))
            .max_w_full()
            .h(px(AI_TEXT_EDITOR_MODAL_HEIGHT))
            .max_h_full()
            .shadow(oxideterm_gpui_ui::theme_overlay_shadow(tokens))
            .flex()
            .flex_col()
            .on_mouse_down(MouseButton::Left, |_event, _window, cx| {
                cx.stop_propagation();
            })
            .on_key_down(|_event, _window, cx| {
                // The root capture layer yields document-editing keys to the
                // focused editor; stop them here after the editor handles them
                // so background panes and shortcuts cannot observe the event.
                cx.stop_propagation();
            })
            .child(
                dialog_header(tokens)
                    .child(dialog_title(tokens, i18n.t(title_key)))
                    .child(dialog_description(tokens, i18n.t(description_key))),
            )
            .child(
                // TextEditorView owns the sole document scroll viewport. The
                // modal body only constrains its available layout rectangle.
                oxideterm_gpui_ui::modal::modal_body(tokens)
                    .flex_1()
                    .min_h(px(0.0))
                    .overflow_hidden()
                    .child(editor),
            )
            .child(
                dialog_footer(tokens)
                    .justify_between()
                    .child(if dialog == AiTextEditorDialog::Memory {
                        Self::toolbar_button(
                            tokens,
                            i18n.t("settings_view.ai.memory_clear"),
                            ToolbarButtonOptions {
                                button: ButtonOptions {
                                    variant: ButtonVariant::Ghost,
                                    size: ButtonSize::Sm,
                                    radius: ButtonRadius::Md,
                                    disabled: false,
                                },
                                ..ToolbarButtonOptions::default()
                            },
                            cx.listener(|this, _event, _window, cx| {
                                if let Some(editor) =
                                    this.draft.as_ref().map(|draft| draft.editor.clone())
                                {
                                    editor.update(cx, |editor, cx| {
                                        editor.replace_text_external(String::new(), cx);
                                    });
                                }
                                cx.stop_propagation();
                                cx.notify();
                            }),
                        )
                        .into_any_element()
                    } else {
                        div().into_any_element()
                    })
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(tokens.metrics.modal_field_gap))
                            .child(Self::toolbar_button(
                                tokens,
                                i18n.t("settings_view.ai.editor_cancel"),
                                ToolbarButtonOptions {
                                    button: ButtonOptions {
                                        variant: ButtonVariant::Outline,
                                        size: ButtonSize::Sm,
                                        radius: ButtonRadius::Md,
                                        disabled: false,
                                    },
                                    ..ToolbarButtonOptions::default()
                                },
                                cx.listener(|this, _event, _window, cx| {
                                    this.dismiss(cx);
                                    cx.stop_propagation();
                                }),
                            ))
                            .child(Self::toolbar_button(
                                tokens,
                                i18n.t("settings_view.ai.editor_save"),
                                ToolbarButtonOptions {
                                    button: ButtonOptions {
                                        variant: ButtonVariant::Default,
                                        size: ButtonSize::Sm,
                                        radius: ButtonRadius::Md,
                                        disabled: false,
                                    },
                                    ..ToolbarButtonOptions::default()
                                },
                                cx.listener(|this, _event, window, cx| {
                                    if let Some(draft) = this.draft.take() {
                                        let text = draft.editor.read(cx).buffer().text();
                                        cx.notify();
                                        (draft.dispatch)(
                                            SettingsWindowRequest::Edit(persisted_text(
                                                draft.kind, text,
                                            )),
                                            window,
                                            cx,
                                        );
                                    }
                                    cx.stop_propagation();
                                }),
                            )),
                    ),
            );

        dismissible_dialog_backdrop()
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _event, _window, cx| {
                    this.dismiss(cx);
                    cx.stop_propagation();
                }),
            )
            .child(overlay_content_boundary(modal))
            .into_any_element()
    }

    pub fn open(
        &mut self,
        dialog: AiTextEditorDialog,
        settings: &PersistedSettings,
        style: AiTextEditorStyle,
        dispatch: SettingsWindowDispatch,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let value = match dialog {
            AiTextEditorDialog::SystemPrompt => settings.ai.custom_system_prompt.clone(),
            AiTextEditorDialog::Memory => settings
                .ai
                .memory
                .entries
                .iter()
                .find(|entry| entry.id == "manual-user-memory")
                .map(|entry| entry.content.clone())
                .unwrap_or_else(|| settings.ai.memory.content.clone()),
        };
        let tokens = style.tokens;
        let i18n = &style.i18n;
        let placeholder = i18n.t(match dialog {
            AiTextEditorDialog::SystemPrompt => "settings_view.ai.system_prompt_placeholder",
            AiTextEditorDialog::Memory => "settings_view.ai.memory_placeholder",
        });
        let context_menu_labels = oxideterm_gpui_editor::EditorContextMenuLabels {
            copy: i18n.t("menu.copy"),
            cut: i18n.t("fileManager.cut"),
            paste: i18n.t("menu.paste"),
            select_all: i18n.t("fileManager.selectAll"),
        };
        let save = dispatch.clone();
        let editor = cx.new(|cx| {
            let mut editor = oxideterm_gpui_editor::TextEditorView::new(value, &tokens, cx);
            let mut editor_settings = oxideterm_gpui_editor::EditorSettings::default();
            editor_settings.soft_wrap = true;
            editor_settings.indentation_markers = false;
            editor_settings.highlight_special_chars = false;
            editor_settings.placeholder = Some(placeholder);
            editor.set_settings(editor_settings, cx);
            editor.set_context_menu_labels(context_menu_labels);
            editor.apply_ide_runtime_settings(
                &tokens,
                style.font_family.clone(),
                style.font_weight,
                style.font_fallback.clone(),
                style.font_size,
                style.line_height,
                true,
                style.background_active,
                cx,
            );
            editor.set_on_save(Box::new(move |text, window, cx| {
                save(
                    SettingsWindowRequest::Edit(persisted_text(dialog, text.to_string())),
                    window,
                    cx,
                );
                Ok(())
            }));
            editor
        });
        let focus_handle = editor.read(cx).focus_handle(cx);
        self.draft = Some(AiTextEditorDraft {
            kind: dialog,
            editor,
            style,
            dispatch,
        });
        window.focus(&focus_handle, cx);
        cx.notify();
    }
}

fn persisted_text(dialog: AiTextEditorDialog, text: String) -> crate::components::SettingsEdit {
    Box::new(move |settings| match dialog {
        AiTextEditorDialog::SystemPrompt => settings.ai.custom_system_prompt = text,
        AiTextEditorDialog::Memory => {
            let now_ms =
                oxideterm_settings_model::current_time_millis().min(i64::MAX as u128) as i64;
            settings.ai.memory.content.clear();
            if let Some(entry) = settings
                .ai
                .memory
                .entries
                .iter_mut()
                .find(|entry| entry.id == "manual-user-memory")
            {
                entry.content = text;
                entry.updated_at_ms = now_ms;
                entry.revision = entry.revision.saturating_add(1);
            } else if !text.trim().is_empty() {
                settings
                    .ai
                    .memory
                    .entries
                    .push(oxideterm_settings::AiMemoryEntry {
                        id: "manual-user-memory".to_string(),
                        content: text,
                        scope_kind: oxideterm_settings::AiMemoryScopeKind::User,
                        scope_id: None,
                        memory_kind: oxideterm_settings::AiMemoryKind::LongTerm,
                        source: oxideterm_settings::AiMemorySource::Manual,
                        created_at_ms: now_ms,
                        updated_at_ms: now_ms,
                        last_used_at_ms: None,
                        use_count: 0,
                        expires_at_ms: None,
                        revision: 1,
                    });
            }
        }
    })
}
