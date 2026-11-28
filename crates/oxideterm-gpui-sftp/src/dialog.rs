// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use super::*;
#[cfg(any(target_os = "macos", target_os = "windows", target_os = "linux"))]
use crate::native_video::sftp_native_video_element;
use crate::style::*;
use gpui::{
    AnyElement, CursorStyle, MouseButton, MouseDownEvent, MouseMoveEvent, ObjectFit, Rgba,
    SharedString, WeakEntity, Window, div, relative, rgb, rgba, svg,
};
use oxideterm_gpui_editor::{EditorContextMenuLabels, EditorSearchLabels};
use oxideterm_gpui_markdown::{MarkdownOptions, render::MarkdownCodeBlockActions};
use oxideterm_gpui_ui::{
    button::{
        ButtonOptions, ButtonRadius, ButtonSize, ButtonVariant, IconButtonOptions,
        ToolbarButtonOptions, icon_button, toolbar_action_button,
    },
    icons::LucideIcon,
    modal::{dismissible_dialog_backdrop, rounded_shell_child_radius},
    surface::{MaterialRole, material_surface},
    text_input::{
        TextInputAnchor, TextInputAnchorId, TextInputContentAlign, TextInputView,
        TextInputViewport, text_input_anchor_probe, text_input_with_viewport_and_content_align,
    },
};
use oxideterm_i18n::I18n;
use oxideterm_sftp::format_file_size;
use oxideterm_theme::ThemeTokens;
use std::{
    hash::{DefaultHasher, Hash, Hasher},
    ops::Range,
    rc::Rc,
};

mod basic;
mod confirm;
mod conflict;
mod content;
mod editor;
mod markdown_font;
mod media;
mod shell;

const SFTP_ICON_SM: f32 = 12.0; // Tauri h-3 w-3
const SFTP_ICON_MD: f32 = 14.0; // Tauri h-3.5 w-3.5
const SFTP_PANEL_80_ALPHA: u32 = 0xcc; // Tauri bg-theme-bg-panel/80
const SFTP_GREEN: u32 = 0x22c55e; // Tauri text-green-500
const SFTP_YELLOW: u32 = 0xeab308; // Tauri text-yellow-500
const SFTP_ORANGE: u32 = 0xfb923c; // Tauri text-orange-400
const SFTP_RED: u32 = 0xf87171; // Tauri text-red-400
const SFTP_DIALOG_SHADOW_ALPHA: u32 = 0x40; // Tauri shadow-lg-ish overlay shadow
const SFTP_DIALOG_BORDER_SUBTLE_ALPHA: u32 = 0x99; // Tauri border-theme-border/60
const SFTP_DIALOG_BORDER_HALF_ALPHA: u32 = 0x80; // Tauri border-theme-border/50
const SFTP_DIALOG_DIVIDER_ALPHA: u32 = 0x66; // Tauri border-theme-border/40
const SFTP_CONFIRM_ICON_BG_ALPHA: u32 = 0x1a; // Tauri bg-theme-accent/10
const SFTP_CONFIRM_ICON_RING_ALPHA: u32 = 0x33; // Tauri ring-theme-accent/20
const SFTP_CONFIRM_ACTION_HOVER_ALPHA: u32 = 0x1a; // Tauri hover:bg-theme-accent/10
const SFTP_EDITOR_RETRY_HOVER_ALPHA: u32 = 0x1a; // Tauri hover:bg-orange-500/10
const SFTP_CONFLICT_NEWER_BG_ALPHA: u32 = 0x4d; // Tauri bg-green-950/30
const SFTP_READONLY_BADGE_BG_ALPHA: u32 = 0x26; // Tauri warning badge translucent fill
const SFTP_DIALOG_WIDTH_XS: f32 = 320.0; // Tauri max-w-xs
const SFTP_DIALOG_WIDTH_SM: f32 = 384.0; // Tauri max-w-sm
const SFTP_DIALOG_WIDTH_LG: f32 = 512.0; // Tauri max-w-lg
const SFTP_DIALOG_WIDTH_4XL: f32 = 896.0; // Tauri max-w-4xl
const SFTP_DIALOG_WIDTH_5XL: f32 = 1024.0; // Tauri max-w-5xl
const SFTP_EDITOR_DIALOG_WIDTH_6XL: f32 = 1152.0; // Tauri max-w-6xl
const SFTP_PREVIEW_DIALOG_HEIGHT_RATIO: f32 = 0.85; // Tauri SFTP preview/editor h-[85vh]
const SFTP_DIFF_DIALOG_HEIGHT_RATIO: f32 = 0.80; // Tauri FileDiffDialog h-[80vh]

pub enum SftpDialogRequest {
    Close,
    Accept,
    Download(String),
    #[cfg(target_family = "wasm")]
    CompareDeviceFile(String),
    OpenExternal(String),
    LoadHex {
        path: String,
        offset: u64,
        generation: u64,
    },
    Resolve(SftpConflictResolution),
    CancelConflicts,
    InputPressed(MouseDownEvent),
    InputDragged(MouseMoveEvent),
}

pub struct SftpDialogIme {
    pub field: Option<Entity<oxideterm_gpui_editor::line_edit::LineField>>,
    pub anchor_id: TextInputAnchorId,
    pub viewport: TextInputViewport,
    pub caret_visible: bool,
    pub selected_range: Option<Range<usize>>,
    pub active_offset: Option<usize>,
    pub marked_text: Option<String>,
    pub update_anchor: Rc<dyn Fn(TextInputAnchor)>,
}

impl SftpDialogIme {
    pub(super) fn mount_field(
        &self,
        source: &Entity<SftpWorkspaceEntity>,
        input: SftpInput,
        pane: Option<SftpPane>,
        value: &str,
        placeholder: String,
        tokens: ThemeTokens,
        cx: &mut App,
    ) -> Option<AnyElement> {
        let field = self.field.as_ref()?;
        field.update(cx, |field, cx| {
            field.sync_value(value, cx);
            field.environment(tokens, placeholder, cx);
        });
        let source = source.clone();
        let control = div()
            .flex_1()
            .min_w_0()
            .h_full()
            .child(field.clone())
            .capture_any_mouse_down(move |event, _, cx| {
                if event.button == MouseButton::Left {
                    source.update(cx, |s, cx| {
                        if let Some(pane) = pane {
                            s.active_pane = pane;
                        }
                        if s.set_input_focus(Some(input), cx) {
                            cx.notify();
                        }
                    });
                }
            });
        let update = self.update_anchor.clone();
        Some(
            text_input_anchor_probe(self.anchor_id, control, move |anchor, _, _| update(anchor))
                .into_any_element(),
        )
    }
}

pub struct SftpPreviewEditorPreferences {
    pub editor_font_family: String,
    pub editor_font_weight: f32,
    pub editor_font_fallback: Option<String>,
    pub editor_font_size: f32,
    pub editor_line_height: f32,
    pub word_wrap: bool,
    pub background_active: bool,
}

type Selectable = Rc<dyn Fn(u64, String, u32) -> AnyElement>;
type Dispatch = Rc<dyn Fn(SftpDialogRequest, &mut Window, &mut App)>;

pub struct SftpDialogFrame {
    pub tokens: ThemeTokens,
    pub i18n: I18n,
    pub markdown_options: MarkdownOptions,
    pub markdown_actions: MarkdownCodeBlockActions,
    pub editor_preferences: SftpPreviewEditorPreferences,
    pub search_labels: EditorSearchLabels,
    pub mono_font: SharedString,
    pub ui_font: SharedString,
    pub ime: SftpDialogIme,
    pub has_background: bool,
    pub selectable: Selectable,
    pub dispatch: Dispatch,
}

pub struct SftpDialogView {
    source: WeakEntity<SftpWorkspaceEntity>,
    frame: SftpDialogFrame,
    _observe: Subscription,
}

impl std::ops::Deref for SftpDialogView {
    type Target = SftpDialogFrame;
    fn deref(&self) -> &Self::Target {
        &self.frame
    }
}

impl SftpDialogView {
    pub fn mount(
        source: &Entity<SftpWorkspaceEntity>,
        frame: SftpDialogFrame,
        cx: &mut App,
    ) -> AnyElement {
        source.update(cx, |state, _| {
            state.ime_overrides.remove(&SftpInput::DialogValue);
        });
        if let Some(view) = source.read(cx).feature_views.dialog.clone() {
            view.update(cx, |view, cx| view.configure(frame, cx));
            return view.into_any_element();
        }
        let view = cx.new(|cx| Self::new(source, frame, cx));
        source.update(cx, |state, _| {
            state.feature_views.dialog = Some(view.clone())
        });
        view.into_any_element()
    }

    fn new(
        source: &Entity<SftpWorkspaceEntity>,
        frame: SftpDialogFrame,
        cx: &mut Context<Self>,
    ) -> Self {
        Self {
            source: source.downgrade(),
            frame,
            _observe: cx.observe(source, |_, _, cx| cx.notify()),
        }
    }

    fn configure(&mut self, frame: SftpDialogFrame, cx: &mut Context<Self>) {
        self.frame = frame;
        cx.notify();
    }

    fn sftp_view(&self) -> Entity<SftpWorkspaceEntity> {
        self.source
            .upgrade()
            .expect("a mounted dialog has a SFTP owner")
    }

    fn sftp_ui_metrics(&self) -> SftpUiMetrics {
        SftpUiMetrics::from_tokens(&self.tokens)
    }

    fn send(&self, request: SftpDialogRequest, window: &mut Window, cx: &mut App) {
        (self.dispatch)(request, window, cx);
    }

    fn close_editor(&self, confirm_if_dirty: bool, cx: &mut App) {
        use oxideterm_gpui_ui::motion::{MotionDuration, duration};
        let delay = duration(&self.tokens, MotionDuration::Control);
        self.sftp_view().update(cx, |state, cx| {
            if !confirm_if_dirty || state.confirm_or_close_editor(cx) {
                state.begin_dialog_exit(delay, cx);
            }
        });
    }

    fn restore_editor(&self, name: String, window: &mut Window, cx: &mut App) {
        self.sftp_view()
            .update(cx, |state, cx| state.restore_editor(name, window, cx));
    }

    fn render_selectable_text_scoped(
        &self,
        scope: &str,
        key: impl Hash,
        text: impl Into<SharedString>,
        color: u32,
        _cx: &mut App,
    ) -> AnyElement {
        let text: String = text.into().to_string();
        (self.selectable)(selectable_id(scope, (key, &text)), text, color)
    }

    fn render_selectable_display_text(
        &self,
        scope: &str,
        key: impl Hash,
        text: String,
        color: u32,
        _cx: &mut App,
    ) -> AnyElement {
        (self.selectable)(
            selectable_id(scope, ((key, text.as_str()), &text)),
            text,
            color,
        )
    }

    fn render_sftp_diff_body(
        &self,
        view: Entity<SftpDiffView>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let selectable = self.selectable.clone();
        view.update(cx, |view, cx| {
            view.configure(
                SftpDiffFrame {
                    tokens: self.tokens,
                    i18n: self.i18n.clone(),
                    mono_font: self.mono_font.clone(),
                    selectable_text: Rc::new(move |scope, path, text, color| {
                        let id = match path {
                            Some(path) => selectable_id(scope, (path, text.as_str())),
                            None => selectable_id(scope, ((), text.as_str())),
                        };
                        selectable(id, text, color)
                    }),
                },
                cx,
            )
        });
        view.into_any_element()
    }

    fn render_sftp_inline_text(
        &self,
        _input: SftpInput,
        _pane: Option<SftpPane>,
        value: &str,
        placeholder_key: &'static str,
        focused: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let source = self.sftp_view();
        if let Some(field) = self.ime.mount_field(
            &source,
            SftpInput::DialogValue,
            None,
            value,
            self.i18n.t(placeholder_key),
            self.tokens,
            cx,
        ) {
            return field;
        }
        let (control, anchor_id, update) = {
            let state = source.read(cx);
            let ime = state
                .ime_overrides
                .get(&SftpInput::DialogValue)
                .unwrap_or(&self.ime);
            let control = text_input_with_viewport_and_content_align(
                &self.tokens,
                TextInputView {
                    value,
                    placeholder: self.i18n.t(placeholder_key),
                    focused,
                    caret_visible: ime.caret_visible,
                    secret: false,
                    selected_all: false,
                    selected_range: ime.selected_range.clone(),
                    marked_text: ime.marked_text.as_deref(),
                },
                TextInputContentAlign::Start,
                &ime.viewport,
                ime.active_offset,
            );
            (control, ime.anchor_id, ime.update_anchor.clone())
        };
        let press_dispatch = self.dispatch.clone();
        let drag_dispatch = self.dispatch.clone();
        let control = control
            .flex_1()
            .min_w(px(0.0))
            .h_full()
            .px(px(0.0))
            .border_0()
            .bg(rgba(0x00000000))
            .text_size(px(self.sftp_ui_metrics().text_xs))
            .cursor(CursorStyle::IBeam)
            .on_mouse_down(
                MouseButton::Left,
                move |event: &MouseDownEvent, window, cx| {
                    source.update(cx, |state, cx| {
                        if state.set_input_focus(Some(SftpInput::DialogValue), cx) {
                            cx.notify();
                        }
                    });
                    (press_dispatch)(SftpDialogRequest::InputPressed(event.clone()), window, cx);
                    cx.stop_propagation();
                },
            )
            .on_mouse_move(move |event: &MouseMoveEvent, window, cx| {
                (drag_dispatch)(SftpDialogRequest::InputDragged(event.clone()), window, cx);
            });
        text_input_anchor_probe(anchor_id, control, move |anchor, _, _| update(anchor))
            .into_any_element()
    }

    fn workspace_toolbar_action_button(
        &self,
        label: String,
        icon: Option<AnyElement>,
        options: ToolbarButtonOptions,
        listener: impl Fn(&MouseDownEvent, &mut Window, &mut App) + 'static,
    ) -> gpui::Div {
        toolbar_action_button(&self.tokens, label, icon, options, listener)
    }

    fn workspace_icon_action_button(
        &self,
        icon: LucideIcon,
        size: f32,
        color: Rgba,
        options: IconButtonOptions,
        listener: impl Fn(&mut Self, &MouseDownEvent, &mut Window, &mut Context<Self>) + 'static,
        cx: &mut Context<Self>,
    ) -> gpui::Div {
        let actionable = !(options.disabled || options.loading);
        icon_button(&self.tokens, render_lucide_icon(icon, size, color), options)
            .when(actionable, |button| {
                button.on_mouse_down(MouseButton::Left, cx.listener(listener))
            })
            .when(!actionable, |button| {
                button.on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            })
    }

    fn can_compare_sftp_preview(&self, name: &str, cx: &App) -> bool {
        self.sftp_view().read(cx).can_compare_preview(name)
    }

    fn can_edit_sftp_preview(&self, cx: &App) -> bool {
        let sftp = self.sftp_view().read(cx);
        sftp.preview_pane_is_editable()
            && matches!(
                sftp.preview_content.as_deref(),
                Some(PreviewContent::Text { .. })
            )
    }

    fn sftp_preview_is_markdown_content(&self, cx: &App) -> bool {
        self.sftp_view().read(cx).preview_is_markdown()
    }

    fn render_sftp_text_button(
        &self,
        label: String,
        primary: bool,
        listener: impl Fn(&MouseDownEvent, &mut Window, &mut App) + 'static,
    ) -> AnyElement {
        let variant = if primary {
            SftpButtonVariant::Default
        } else {
            SftpButtonVariant::Secondary
        };
        self.render_sftp_button_variant(label, variant, listener)
    }

    fn render_sftp_button_variant(
        &self,
        label: String,
        variant: SftpButtonVariant,
        listener: impl Fn(&MouseDownEvent, &mut Window, &mut App) + 'static,
    ) -> AnyElement {
        crate::style::render_sftp_button_variant(&self.tokens, label, variant, listener)
    }
}

impl Render for SftpDialogView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let Some(dialog) = self
            .source
            .upgrade()
            .and_then(|source| source.read(cx).dialog())
        else {
            return div().size(px(0.0)).into_any_element();
        };
        self.render_sftp_dialog(dialog, self.has_background, cx)
    }
}

fn selectable_id(scope: &str, value: impl Hash) -> u64 {
    let mut hasher = DefaultHasher::new();
    scope.hash(&mut hasher);
    value.hash(&mut hasher);
    hasher.finish()
}

fn render_lucide_icon(icon: LucideIcon, size: f32, color: Rgba) -> AnyElement {
    svg()
        .path(icon.path())
        .size(px(size))
        .text_color(color)
        .into_any_element()
}

fn format_conflict_modified(modified: Option<i64>) -> String {
    modified
        .and_then(|time| chrono::DateTime::from_timestamp(time, 0))
        .map(|time| {
            time.with_timezone(&chrono::Local)
                .format("%Y/%-m/%-d %-H:%M:%S")
                .to_string()
        })
        .unwrap_or_else(|| "Unknown".to_owned())
}

fn format_sftp_media_time(duration: Duration) -> String {
    let total = duration.as_secs();
    format!("{}:{:02}", total / 60, total % 60)
}

trait PreviewScrollExt: StatefulInteractiveElement + Sized {
    fn selectable_overflow_y_scroll(self, handle: &ScrollHandle) -> Self {
        self.overflow_y_scroll().track_scroll(handle)
    }
}

impl<T: StatefulInteractiveElement> PreviewScrollExt for T {}
