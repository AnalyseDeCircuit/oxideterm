use std::{
    hash::{DefaultHasher, Hash, Hasher},
    rc::Rc,
};

use gpui::{
    AnyElement, App, Div, Entity, MouseButton, MouseDownEvent, Window, div, prelude::*, px, rgb,
    rgba,
};
use oxideterm_gpui_ui::{
    button::{
        ButtonOptions, ButtonRadius, ButtonSize, ButtonVariant, IconButtonOptions,
        ToolbarButtonIconPosition, ToolbarButtonOptions, icon_button, toolbar_button,
    },
    confirm::{ConfirmDialogVariant, ConfirmDialogView},
    modal::rounded_shell_child_radius,
    text_input::TextInputContentAlign,
};
use oxideterm_settings_model::SettingsSectionId;

use crate::{
    SettingsKeybindingScopeFilter,
    components::{
        SettingsComponents, SettingsImeFrame, SettingsWindowDispatch, settings_mono_font_family,
    },
    entity::{KeybindingRecordingFooterAction, SettingsWorkspaceEntity},
    keybindings::{ActionDefinition, ActionScope, KeybindingSide, effective_combo, format_combo},
};

pub enum KeybindingRequest {
    Import,
    Export,
    Reset(String),
    Unbind(String),
    CommitRecording,
    CloseReset { apply: bool },
}

#[derive(Clone, Copy)]
pub enum KeybindingTextRole {
    PlainDocument,
    NonSelectable,
}

pub struct KeybindingPage<'a> {
    pub components: SettingsComponents<'a>,
    pub state: &'a Entity<SettingsWorkspaceEntity>,
    pub catalog: &'a [ActionDefinition],
    pub search: &'a dyn Fn(&App) -> AnyElement,
    pub label: &'a dyn Fn(KeybindingTextRole, u64, String, u32) -> AnyElement,
    pub dispatch: Rc<dyn Fn(KeybindingRequest, &mut App)>,
}

impl<'a> std::ops::Deref for KeybindingPage<'a> {
    type Target = SettingsComponents<'a>;
    fn deref(&self) -> &Self::Target {
        &self.components
    }
}

fn render_icon(path: &'static str, size: f32, color: gpui::Rgba) -> AnyElement {
    gpui::svg()
        .path(path)
        .size(px(size))
        .text_color(color)
        .into_any_element()
}

impl SettingsComponents<'_> {
    pub fn keybinding_search_field(
        &self,
        frame: SettingsImeFrame<'_>,
        dispatch: SettingsWindowDispatch,
    ) -> AnyElement {
        self.text_field(
            frame,
            self.i18n.t("settings_view.keybindings.search_placeholder"),
            TextInputContentAlign::Start,
            false,
            |field| {
                field.w(px(280.0)).h(px(36.0)).pl(px(34.0)).child(
                    div()
                        .absolute()
                        .left(px(12.0))
                        .top(px(10.0))
                        .child(render_icon(
                            "lucide/search.svg",
                            15.0,
                            rgb(self.tokens.ui.text_muted),
                        )),
                )
            },
            dispatch,
        )
    }
}

impl KeybindingPage<'_> {
    fn render_display_text_with_role(
        &self,
        role: KeybindingTextRole,
        scope: &str,
        key: impl Hash,
        value: String,
        color: u32,
        _: &App,
    ) -> AnyElement {
        let mut hasher = DefaultHasher::new();
        scope.hash(&mut hasher);
        (key, value.as_str()).hash(&mut hasher);
        (self.label)(role, hasher.finish(), value, color)
    }

    fn toolbar_action_button(
        &self,
        label: String,
        icon: Option<AnyElement>,
        options: ToolbarButtonOptions,
        listener: impl Fn(&MouseDownEvent, &mut Window, &mut App) + 'static,
    ) -> Div {
        let actionable = !(options.button.disabled || options.loading);
        toolbar_button(self.tokens, label, icon, options)
            .when(actionable, |button| {
                button.on_mouse_down(MouseButton::Left, listener)
            })
            .when(!actionable, |button| {
                button.on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            })
    }

    fn icon_action_button(
        &self,
        icon: &'static str,
        size: f32,
        color: gpui::Rgba,
        options: IconButtonOptions,
        listener: impl Fn(&MouseDownEvent, &mut Window, &mut App) + 'static,
        _: &App,
    ) -> Div {
        icon_button(self.tokens, render_icon(icon, size, color), options)
            .on_mouse_down(MouseButton::Left, listener)
    }

    fn request<R: Fn() -> KeybindingRequest + 'static>(
        &self,
        request: R,
    ) -> impl Fn(&MouseDownEvent, &mut Window, &mut App) + use<R> {
        let dispatch = self.dispatch.clone();
        move |_, _, cx| {
            dispatch(request(), cx);
            cx.stop_propagation();
        }
    }

    fn recording_footer(
        &self,
        action: KeybindingRecordingFooterAction,
    ) -> impl Fn(&MouseDownEvent, &mut Window, &mut App) + use<> {
        let state = self.state.downgrade();
        let dispatch = self.dispatch.clone();
        move |_, _, cx| {
            if state
                .update(cx, |state, cx| {
                    state.activate_keybinding_recording_footer(action, cx)
                })
                .unwrap_or(false)
            {
                dispatch(KeybindingRequest::CommitRecording, cx);
            }
            cx.stop_propagation();
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum KeybindingToolbarAction {
    Import,
    Export,
    ResetAll,
}

const KEYBINDING_BG_ACTIVE_ELEVATED_ALPHA: u32 = 0x73; // Tauri [data-bg-active] --color-theme-bg-elevated: 45%.
const KEYBINDING_BG_ACTIVE_BORDER_ALPHA: u32 = 0xbf; // Tauri [data-bg-active] --color-theme-border: 75%.
const KEYBINDING_HEADER_BG_ALPHA: u32 = 0x80; // Tauri bg-theme-bg-elevated/50.
const KEYBINDING_ROW_DIVIDER_ALPHA: u32 = 0x4d; // Tauri divide-theme-border/30.
const KEYBINDING_KBD_BORDER_ALPHA: u32 = 0x80; // Tauri border-theme-border/50.

pub fn settings_keybinding_scope_matches(
    filter: SettingsKeybindingScopeFilter,
    scope: ActionScope,
) -> bool {
    match filter {
        SettingsKeybindingScopeFilter::Files => scope == ActionScope::FileManager,
        SettingsKeybindingScopeFilter::Preview => scope == ActionScope::Preview,
        SettingsKeybindingScopeFilter::RemoteDesktop => scope == ActionScope::RemoteDesktop,
        SettingsKeybindingScopeFilter::Plugins => scope == ActionScope::Plugin,
        SettingsKeybindingScopeFilter::AiPanel => scope == ActionScope::AiPanel,
        SettingsKeybindingScopeFilter::All => true,
        SettingsKeybindingScopeFilter::Editor => scope == ActionScope::Editor,
        SettingsKeybindingScopeFilter::Sftp => scope == ActionScope::Sftp,
        SettingsKeybindingScopeFilter::Global => scope == ActionScope::Global,
        SettingsKeybindingScopeFilter::Terminal => scope == ActionScope::Terminal,
        SettingsKeybindingScopeFilter::Split => scope == ActionScope::Split,
        SettingsKeybindingScopeFilter::Palette => scope == ActionScope::Palette,
    }
}

impl KeybindingToolbarAction {
    fn label_key(self) -> &'static str {
        match self {
            Self::Import => "settings_view.keybindings.import",
            Self::Export => "settings_view.keybindings.export",
            Self::ResetAll => "settings_view.keybindings.reset_all",
        }
    }

    fn icon(self) -> &'static str {
        match self {
            Self::Import => "lucide/upload.svg",
            Self::Export => "lucide/download.svg",
            Self::ResetAll => "lucide/rotate-ccw.svg",
        }
    }

    fn destructive(self) -> bool {
        matches!(self, Self::ResetAll)
    }
}

impl KeybindingPage<'_> {
    fn keybinding_label(&self, definition: &ActionDefinition) -> String {
        definition
            .label
            .clone()
            .unwrap_or_else(|| self.i18n.t(&definition.label_key()))
    }

    fn keybinding_surface_border(&self) -> gpui::Rgba {
        oxideterm_gpui_ui::color_for_background(
            self.tokens.ui.border,
            self.has_background,
            KEYBINDING_BG_ACTIVE_BORDER_ALPHA,
        )
    }

    fn keybinding_row_divider(&self) -> gpui::Rgba {
        // Tauri composes divide-theme-border/30 with the [data-bg-active]
        // border variable, so the slash opacity must scale after the 75% mix.
        oxideterm_gpui_ui::color_with_background_scaled_alpha(
            self.tokens.ui.border,
            self.has_background,
            KEYBINDING_ROW_DIVIDER_ALPHA,
            KEYBINDING_BG_ACTIVE_BORDER_ALPHA,
        )
    }

    fn keybinding_header_background(&self) -> gpui::Rgba {
        // Source: KeybindingEditorSection `bg-theme-bg-elevated/50`, whose
        // theme variable becomes 45% opaque under [data-bg-active].
        oxideterm_gpui_ui::color_with_background_scaled_alpha(
            self.tokens.ui.bg_elevated,
            self.has_background,
            KEYBINDING_HEADER_BG_ALPHA,
            KEYBINDING_BG_ACTIVE_ELEVATED_ALPHA,
        )
    }

    fn keybinding_hover_background(&self) -> gpui::Rgba {
        oxideterm_gpui_ui::color_for_background(self.tokens.ui.bg_hover, self.has_background, 0x80)
    }

    pub fn settings_keybindings_section(&self, section: SettingsSectionId, cx: &App) -> AnyElement {
        let settings = self.settings;
        let modified = settings.keybindings.overrides.len();
        let section_index = match section {
            SettingsSectionId::KeybindingsToolbar => return self.keybinding_toolbar(modified, cx),
            SettingsSectionId::KeybindingsScope(scope_index) => scope_index + 1,
            _ => return div().into_any_element(),
        };

        let side = KeybindingSide::current();
        let query = self
            .state
            .read(cx)
            .keybinding_search_query()
            .trim()
            .to_lowercase();
        let scope_filter = self.state.read(cx).keybinding_scope_filter();
        let mut visible_index = 0;
        for scope in [
            ActionScope::Global,
            ActionScope::Terminal,
            ActionScope::Split,
            ActionScope::Palette,
            ActionScope::Editor,
            ActionScope::Sftp,
            ActionScope::FileManager,
            ActionScope::Preview,
            ActionScope::RemoteDesktop,
            ActionScope::Plugin,
            ActionScope::AiPanel,
        ] {
            let definitions = self
                .catalog
                .iter()
                .filter(|definition| definition.scope == scope)
                .filter(|definition| {
                    settings_keybinding_scope_matches(scope_filter, definition.scope)
                })
                .filter(|definition| {
                    if query.is_empty() {
                        return true;
                    }
                    let label = self.keybinding_label(definition).to_lowercase();
                    label.contains(&query) || definition.id.to_lowercase().contains(&query)
                })
                .collect::<Vec<_>>();
            if !definitions.is_empty() {
                visible_index += 1;
                if visible_index == section_index {
                    return self.keybinding_scope_table(scope, &definitions, side, cx);
                }
            }
        }

        if section_index == 1 && visible_index == 0 {
            return self.keybinding_no_results(cx);
        }

        div().into_any_element()
    }

    fn keybinding_toolbar(&self, modified: usize, cx: &App) -> AnyElement {
        div()
            .w_full()
            .min_w(px(0.0))
            .flex()
            .flex_col()
            .gap(px(12.0))
            .child(
                div()
                    .w_full()
                    .min_w(px(0.0))
                    .flex()
                    .flex_row()
                    .flex_wrap()
                    .items_center()
                    .gap(px(12.0))
                    .child((self.search)(cx))
                    .child(self.keybinding_scope_filter(cx))
                    .child(div().flex_1().min_w(px(0.0)))
                    .child(self.keybinding_toolbar_button(KeybindingToolbarAction::Import, false))
                    .child(self.keybinding_toolbar_button(KeybindingToolbarAction::Export, false))
                    .when(modified > 0, |toolbar| {
                        toolbar.child(self.keybinding_toolbar_button(
                            KeybindingToolbarAction::ResetAll,
                            modified == 0,
                        ))
                    }),
            )
            .into_any_element()
    }

    fn keybinding_scope_filter(&self, cx: &App) -> AnyElement {
        let selected = self.state.read(cx).keybinding_scope_filter();
        let mut filters = div().flex().flex_wrap().gap(px(4.0)).max_w_full();
        for filter in SettingsKeybindingScopeFilter::all().iter().copied() {
            let active = filter == selected;
            let item = oxideterm_gpui_ui::segmented_control_item(
                self.tokens,
                self.i18n.t(filter.label_key()),
                active,
            )
            .flex_none()
            .when(active, |item| item.bg(self.keybinding_hover_background()))
            .on_mouse_down(MouseButton::Left, {
                let state = self.state.downgrade();
                move |_, _, cx| {
                    let _ = state.update(cx, |settings, cx| {
                        settings.set_keybinding_scope_filter(filter, cx)
                    });
                    cx.stop_propagation();
                }
            });
            filters = filters.child(item);
        }
        filters.into_any_element()
    }

    fn keybinding_toolbar_button(
        &self,
        action: KeybindingToolbarAction,
        disabled: bool,
    ) -> AnyElement {
        let icon_color = if action.destructive() {
            self.tokens.ui.error
        } else {
            self.tokens.ui.text_muted
        };
        let hover_text_color = if action.destructive() {
            rgb(self.tokens.ui.error)
        } else {
            rgb(self.tokens.ui.text)
        };

        // Tauri renders keybinding toolbar actions as shadcn ghost Buttons
        // with leading lucide icons. Use the workspace action guard so disabled
        // toolbar buttons cannot dispatch pointer activation in native either.
        self.toolbar_action_button(
            self.i18n.t(action.label_key()),
            Some(render_icon(action.icon(), 14.0, rgb(icon_color))),
            ToolbarButtonOptions {
                button: ButtonOptions {
                    variant: ButtonVariant::Ghost,
                    size: ButtonSize::Sm,
                    radius: ButtonRadius::Md,
                    disabled,
                },
                icon_position: ToolbarButtonIconPosition::Leading,
                text_color: action.destructive().then(|| rgb(self.tokens.ui.error)),
                hover_text_color: Some(hover_text_color),
                ..ToolbarButtonOptions::default()
            },
            {
                let state = self.state.downgrade();
                let dispatch = self.dispatch.clone();
                move |_, _, cx| {
                    match action {
                        KeybindingToolbarAction::Import => dispatch(KeybindingRequest::Import, cx),
                        KeybindingToolbarAction::Export => dispatch(KeybindingRequest::Export, cx),
                        KeybindingToolbarAction::ResetAll => {
                            let _ = state.update(cx, |settings, cx| {
                                settings.open_keybinding_reset_confirm(cx)
                            });
                        }
                    }
                    cx.stop_propagation();
                }
            },
        )
        .into_any_element()
    }

    fn keybinding_no_results(&self, cx: &App) -> AnyElement {
        div()
            .w_full()
            .py(px(44.0))
            .flex()
            .items_center()
            .justify_center()
            .text_size(px(self.tokens.metrics.ui_text_sm))
            .text_color(rgb(self.tokens.ui.text_muted))
            .child(self.render_display_text_with_role(
                KeybindingTextRole::PlainDocument,
                "settings-keybindings",
                "no-results",
                self.i18n.t("settings_view.keybindings.no_results"),
                self.tokens.ui.text_muted,
                cx,
            ))
            .into_any_element()
    }

    fn keybinding_scope_table(
        &self,
        scope: ActionScope,
        definitions: &[&ActionDefinition],
        side: KeybindingSide,
        cx: &App,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let table_surface = div()
            .w_full()
            .min_w(px(0.0))
            .rounded(px(self.tokens.radii.lg))
            .border_1()
            .border_color(self.keybinding_surface_border())
            .overflow_hidden()
            .child(
                div()
                    .h(px(40.0))
                    .px(px(14.0))
                    .flex()
                    .items_center()
                    .justify_between()
                    // Tauri relies on `overflow-hidden` on the rounded table.
                    // GPUI can leave child paint visible at the mask edge, so
                    // round the header explicitly to preserve the browser clip.
                    .rounded_t(px(rounded_shell_child_radius(self.tokens.radii.lg)))
                    .bg(self.keybinding_header_background())
                    .border_b_1()
                    .border_color(self.keybinding_surface_border())
                    .child(
                        div()
                            .text_size(px(self.tokens.metrics.ui_text_xs))
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .text_color(rgb(theme.text_muted))
                            .child(self.render_display_text_with_role(
                                KeybindingTextRole::PlainDocument,
                                "settings-keybindings-scope",
                                scope.label_key(),
                                self.i18n.t(scope.label_key()),
                                theme.text_muted,
                                cx,
                            )),
                    )
                    .child(
                        div()
                            .text_size(px(self.tokens.metrics.ui_text_xs))
                            .text_color(rgb(theme.text_muted))
                            .child(self.render_display_text_with_role(
                                KeybindingTextRole::PlainDocument,
                                "settings-keybindings-column",
                                "shortcut",
                                self.i18n.t("settings_view.keybindings.column_shortcut"),
                                theme.text_muted,
                                cx,
                            )),
                    ),
            );

        let mut table = self.settings_card_surface(table_surface, theme.bg_card);

        for (index, definition) in definitions.iter().enumerate() {
            table = table.child(self.keybinding_action_row(
                definition,
                side,
                index + 1 == definitions.len(),
                cx,
            ));
        }

        table.into_any_element()
    }

    fn keybinding_action_row(
        &self,
        definition: &ActionDefinition,
        side: KeybindingSide,
        is_last: bool,
        cx: &App,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let settings = self.settings;
        let current = effective_combo(definition, &settings.keybindings.overrides, side);
        let default = definition.default_combo(side);
        let modified = current.as_ref() != Some(default);
        let recording = self
            .state
            .read(cx)
            .keybinding_recording_action_id()
            .is_some_and(|id| id == definition.id);
        let action_id = definition.id.to_string();
        let record_action_id = action_id.clone();
        let unbind_action_id = action_id.clone();
        let reset_action_id = action_id;
        let conflicts = if recording {
            self.state.read(cx).keybinding_conflicts().to_vec()
        } else {
            Vec::new()
        };

        div()
            .w_full()
            .min_w(px(0.0))
            .px(px(20.0))
            .py(px(12.0))
            .flex()
            .items_center()
            .justify_between()
            .gap(px(12.0))
            // Tauri's `divide-y` does not draw a divider after the last row.
            // Avoid a final border inside the rounded bottom because GPUI's
            // rounded overflow can otherwise expose the line outside the mask.
            .when(!is_last, |row| {
                row.border_b_1().border_color(self.keybinding_row_divider())
            })
            .when(is_last, |row| {
                row.rounded_b(px(rounded_shell_child_radius(self.tokens.radii.lg)))
            })
            .when(recording, |row| row.bg(rgba((theme.accent << 8) | 0x0d)))
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.0))
                    .flex()
                    .items_center()
                    .gap(px(12.0))
                    .child(
                        div()
                            .truncate()
                            .text_size(px(self.tokens.metrics.ui_text_sm))
                            .font_weight(gpui::FontWeight::MEDIUM)
                            .text_color(rgb(theme.text))
                            .child(self.render_display_text_with_role(
                                KeybindingTextRole::PlainDocument,
                                "settings-keybinding-action",
                                definition.id.as_ref(),
                                self.keybinding_label(definition),
                                theme.text,
                                cx,
                            )),
                    )
                    .when(modified, |label| {
                        label.child(self.keybinding_modified_badge(cx))
                    }),
            )
            .child(
                div()
                    .flex_none()
                    .flex()
                    .items_center()
                    .gap(px(8.0))
                    .when(recording, |controls| {
                        controls.child(self.keybinding_recording_cell(&conflicts, side, cx))
                    })
                    .when(!recording, |controls| {
                        controls
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap(px(4.0))
                                    .rounded(px(self.tokens.radii.sm))
                                    .px(px(8.0))
                                    .py(px(4.0))
                                    .cursor_pointer()
                                    .hover(|style| style.bg(self.keybinding_hover_background()))
                                    .child(self.keybinding_kbd_badge(
                                        &current.as_ref().map(format_combo).unwrap_or_else(|| {
                                            self.i18n.t("settings_view.keybindings.unbound")
                                        }),
                                        false,
                                        cx,
                                    ))
                                    .on_mouse_down(MouseButton::Left, {
                                        let state = self.state.downgrade();
                                        move |_, _, cx| {
                                            let _ = state.update(cx, |settings, cx| {
                                                settings.start_keybinding_recording(
                                                    record_action_id.clone(),
                                                    cx,
                                                )
                                            });
                                            cx.stop_propagation();
                                        }
                                    }),
                            )
                            .when(current.is_some(), |controls| {
                                controls.child(self.icon_action_button(
                                    "lucide/trash-2.svg",
                                    14.0,
                                    rgb(self.tokens.ui.text_muted),
                                    IconButtonOptions {
                                        hover_background: Some(self.keybinding_hover_background()),
                                        ..IconButtonOptions::opaque_toolbar(28.0, ButtonRadius::Sm)
                                    },
                                    self.request(move || {
                                        KeybindingRequest::Unbind(unbind_action_id.clone())
                                    }),
                                    cx,
                                ))
                            })
                            .when(modified, |controls| {
                                controls.child(self.icon_action_button(
                                    "lucide/rotate-ccw.svg",
                                    14.0,
                                    rgb(self.tokens.ui.text_muted),
                                    IconButtonOptions {
                                        hover_background: Some(self.keybinding_hover_background()),
                                        // KeybindingEditorSection renders this
                                        // as a compact icon Button. Use the
                                        // shared icon guard instead of a local
                                        // wrapper so reset actions cannot drift.
                                        ..IconButtonOptions::opaque_toolbar(28.0, ButtonRadius::Sm)
                                    },
                                    self.request(move || {
                                        KeybindingRequest::Reset(reset_action_id.clone())
                                    }),
                                    cx,
                                ))
                            })
                    }),
            )
            .into_any_element()
    }

    fn keybinding_recording_cell(
        &self,
        conflicts: &[String],
        side: KeybindingSide,
        cx: &App,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let combo_display = self
            .state
            .read(cx)
            .keybinding_recording_combo()
            .map(format_combo);
        div()
            .flex()
            .items_center()
            .gap(px(8.0))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .items_end()
                    .gap(px(4.0))
                    .child(match combo_display.as_deref() {
                        Some(combo) => self.keybinding_kbd_badge(combo, true, cx),
                        None => div()
                            .text_size(px(self.tokens.metrics.ui_text_xs))
                            .italic()
                            .text_color(rgb(theme.text_muted))
                            .child(self.render_display_text_with_role(
                                KeybindingTextRole::PlainDocument,
                                "settings-keybindings-record",
                                "prompt",
                                self.i18n.t("settings_view.keybindings.record_prompt"),
                                theme.text_muted,
                                cx,
                            ))
                            .into_any_element(),
                    })
                    .when(combo_display.is_some() && !conflicts.is_empty(), |cell| {
                        cell.child(
                            div()
                                .max_w(px(240.0))
                                .truncate()
                                .text_size(px(self.tokens.metrics.ui_text_2xs))
                                .text_color(rgb(theme.warning))
                                .child(self.render_display_text_with_role(
                                    KeybindingTextRole::PlainDocument,
                                    "settings-keybindings-conflict",
                                    conflicts.join("|"),
                                    self.keybinding_conflict_text(conflicts, side, cx),
                                    theme.warning,
                                    cx,
                                )),
                        )
                    }),
            )
            .when(combo_display.is_some(), |cell| {
                let label_key = if conflicts.is_empty() {
                    "✓"
                } else {
                    "settings_view.keybindings.override_anyway"
                };
                cell.child(self.keybinding_recording_confirm_button(
                    if conflicts.is_empty() {
                        label_key.to_string()
                    } else {
                        self.i18n.t(label_key)
                    },
                    conflicts.is_empty(),
                    cx,
                ))
            })
            .child(self.keybinding_recording_cancel_button(cx))
            .into_any_element()
    }

    fn keybinding_modified_badge(&self, cx: &App) -> AnyElement {
        div()
            .flex_none()
            .rounded(px(self.tokens.radii.sm))
            .bg(rgba((self.tokens.ui.accent << 8) | 0x33))
            .px(px(6.0))
            .py(px(1.0))
            .text_size(px(self.tokens.metrics.ui_text_2xs))
            .font_weight(gpui::FontWeight::MEDIUM)
            .text_color(rgb(self.tokens.ui.accent))
            .child(self.render_display_text_with_role(
                KeybindingTextRole::PlainDocument,
                "settings-keybindings",
                "modified",
                self.i18n.t("settings_view.keybindings.modified"),
                self.tokens.ui.accent,
                cx,
            ))
            .into_any_element()
    }

    fn keybinding_kbd_badge(&self, value: &str, accent: bool, cx: &App) -> AnyElement {
        div()
            .rounded(px(self.tokens.radii.sm))
            .border_1()
            .border_color(if accent {
                rgba((self.tokens.ui.accent << 8) | 0x4d)
            } else {
                oxideterm_gpui_ui::color_with_background_scaled_alpha(
                    self.tokens.ui.border,
                    self.has_background,
                    KEYBINDING_KBD_BORDER_ALPHA,
                    KEYBINDING_BG_ACTIVE_BORDER_ALPHA,
                )
            })
            .bg(if accent {
                rgba((self.tokens.ui.accent << 8) | 0x33)
            } else {
                self.settings_panel_background(self.tokens.ui.bg)
            })
            .px(px(8.0))
            .py(px(2.0))
            .font_family(settings_mono_font_family(self.settings))
            .text_size(px(self.tokens.metrics.ui_text_xs))
            .text_color(rgb(if accent {
                self.tokens.ui.accent
            } else {
                self.tokens.ui.text
            }))
            .child(self.render_display_text_with_role(
                // Shortcut chips live inside clickable controls. Browser buttons
                // do not start document text selection from their label, and
                // keeping these chips non-selectable avoids extra anchor probes
                // while the settings page scrolls.
                KeybindingTextRole::NonSelectable,
                "settings-keybinding-chip",
                (value, accent),
                value.to_string(),
                if accent {
                    self.tokens.ui.accent
                } else {
                    self.tokens.ui.text
                },
                cx,
            ))
            .into_any_element()
    }

    fn keybinding_recording_confirm_button(
        &self,
        label: String,
        is_clean_combo: bool,
        cx: &App,
    ) -> AnyElement {
        let text_color = if is_clean_combo {
            self.tokens.ui.accent
        } else {
            self.tokens.ui.warning
        };

        self.toolbar_action_button(
            label,
            None,
            ToolbarButtonOptions {
                button: ButtonOptions {
                    variant: ButtonVariant::Ghost,
                    size: ButtonSize::Sm,
                    radius: ButtonRadius::Md,
                    disabled: false,
                },
                text_color: Some(rgb(text_color)),
                hover_text_color: Some(rgb(text_color)),
                // Browser focus rings on Tauri's RecordingCell buttons are
                // keyboard-origin only; mouse activation below clears this
                // footer focus instead of synthesizing a focus-visible state.
                focus_visible: self.state.read(cx).keybinding_recording_footer_focus()
                    == Some(KeybindingRecordingFooterAction::Confirm),
                ..ToolbarButtonOptions::default()
            },
            self.recording_footer(KeybindingRecordingFooterAction::Confirm),
        )
        .into_any_element()
    }

    fn keybinding_recording_cancel_button(&self, cx: &App) -> AnyElement {
        self.icon_action_button(
            "lucide/x.svg",
            14.0,
            rgb(self.tokens.ui.text_muted),
            IconButtonOptions {
                hover_background: Some(rgb(self.tokens.ui.bg_hover)),
                focus_visible: self.state.read(cx).keybinding_recording_footer_focus()
                    == Some(KeybindingRecordingFooterAction::Cancel),
                ..IconButtonOptions::opaque_toolbar(28.0, ButtonRadius::Sm)
            },
            self.recording_footer(KeybindingRecordingFooterAction::Cancel),
            cx,
        )
        .into_any_element()
    }

    fn keybinding_conflict_text(
        &self,
        conflicts: &[String],
        side: KeybindingSide,
        _: &App,
    ) -> String {
        let Some(conflict) = conflicts
            .iter()
            .filter_map(|id| {
                self.catalog
                    .iter()
                    .find(|definition| definition.id.as_ref() == id.as_str())
            })
            .next()
        else {
            return String::new();
        };
        self.i18n
            .t("settings_view.keybindings.conflict_warning")
            .replace("{{scope}}", &self.i18n.t(conflict.scope.label_key()))
            .replace("{{action}}", &self.keybinding_label(conflict))
            .replace("{{shortcut}}", &format_combo(conflict.default_combo(side)))
    }

    pub fn render_keybinding_reset_all_confirm_dialog(&self, cx: &App) -> AnyElement {
        let confirm = self
            .state
            .read(cx)
            .keybinding_reset_confirm_snapshot()
            .expect("keybinding reset confirmation must be open while rendered");
        oxideterm_gpui_ui::confirm::confirm_dialog_with_focus_motion(
            self.tokens,
            "settings-keybindings-reset-all-confirm-motion",
            confirm.phase,
            ConfirmDialogView {
                variant: ConfirmDialogVariant::Danger,
                title: div()
                    .child(self.render_display_text_with_role(
                        KeybindingTextRole::PlainDocument,
                        "settings-keybindings-reset-dialog",
                        "title",
                        self.i18n.t("settings_view.keybindings.reset_all_confirm"),
                        self.tokens.ui.text_heading,
                        cx,
                    ))
                    .into_any_element(),
                description: None,
                cancel_label: div()
                    // Dialog action labels mirror browser select-none buttons.
                    .child(self.render_display_text_with_role(
                        KeybindingTextRole::NonSelectable,
                        "settings-keybindings-reset-dialog",
                        "cancel",
                        self.i18n.t("common.actions.cancel"),
                        self.tokens.ui.text,
                        cx,
                    ))
                    .into_any_element(),
                confirm_label: div()
                    .child(self.render_display_text_with_role(
                        KeybindingTextRole::NonSelectable,
                        "settings-keybindings-reset-dialog",
                        "confirm",
                        self.i18n.t("settings_view.keybindings.reset_all"),
                        self.tokens.ui.text,
                        cx,
                    ))
                    .into_any_element(),
            },
            confirm.focused_action,
            self.request(|| KeybindingRequest::CloseReset { apply: false }),
            self.request(|| KeybindingRequest::CloseReset { apply: true }),
        )
    }
}
