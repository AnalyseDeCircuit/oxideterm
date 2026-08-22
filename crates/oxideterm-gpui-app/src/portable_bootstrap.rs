// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

//! Password-gated startup for the self-contained portable runtime.

use std::ops::Range;

use gpui::{
    App, AppContext, Bounds, Context, CursorStyle, Element, ElementId, FocusHandle, Focusable,
    GlobalElementId, InputHandler, InspectorElementId, InteractiveElement, IntoElement,
    KeyDownEvent, LayoutId, MouseButton, ParentElement, Pixels, Render, SharedString, Styled, Task,
    UTF16Selection, Window, WindowDecorations, div, prelude::FluentBuilder, px, rgb, rgba,
};
use oxideterm_gpui_ui::{ButtonTone, TextInputView, button, text_input, text_input_anchor_probe};
use oxideterm_i18n::I18n;
use oxideterm_portable_runtime::{PortableBootstrapStatus, PortableStatusSnapshot};
use oxideterm_settings::{PersistedSettings, WindowUiState};
use oxideterm_theme::ThemeTokens;
use zeroize::{Zeroize, Zeroizing};

use crate::single_instance::SingleInstanceReceiver;
use crate::workspace::{locale_from_settings, portable_bootstrap_tokens_from_settings};

#[derive(Clone, Copy, Eq, PartialEq)]
enum PortableBootstrapInput {
    Password,
    ConfirmPassword,
}

#[derive(Clone, Copy, Eq, PartialEq)]
enum PortableBootstrapAction {
    Create,
    Unlock,
}

struct PortableBootstrapLaunch {
    native_connection_launch: Option<oxideterm_ssh_launch::NativeConnectionLaunch>,
    desktop_presence_menu: oxideterm_desktop_presence::DesktopPresenceMenu,
    single_instance_rx: Option<SingleInstanceReceiver>,
    window_ui: WindowUiState,
}

/// Owns portable password drafts until the encrypted keystore accepts them.
///
/// This type intentionally has no `Debug` implementation. Both drafts zeroize
/// on replacement, successful handoff, window closure, and entity release.
struct PortableBootstrapWindow {
    status: PortableStatusSnapshot,
    i18n: I18n,
    tokens: ThemeTokens,
    focus_handle: FocusHandle,
    active_input: PortableBootstrapInput,
    password: Zeroizing<String>,
    confirm_password: Zeroizing<String>,
    selection: Range<usize>,
    marked_range: Option<Range<usize>>,
    password_bounds: Option<Bounds<Pixels>>,
    confirm_password_bounds: Option<Bounds<Pixels>>,
    pending_action: Option<PortableBootstrapAction>,
    error: Option<String>,
    launch: Option<PortableBootstrapLaunch>,
    action_task: Option<Task<()>>,
}

pub(crate) fn portable_startup_requires_bootstrap(status: PortableBootstrapStatus) -> bool {
    matches!(
        status,
        PortableBootstrapStatus::NeedsSetup | PortableBootstrapStatus::Locked
    )
}

pub(crate) fn open_portable_bootstrap_window(
    cx: &mut App,
    status: PortableStatusSnapshot,
    settings: PersistedSettings,
    native_connection_launch: Option<oxideterm_ssh_launch::NativeConnectionLaunch>,
    desktop_presence_menu: oxideterm_desktop_presence::DesktopPresenceMenu,
    single_instance_rx: Option<SingleInstanceReceiver>,
) -> anyhow::Result<()> {
    let window_ui = settings.window_ui.clone();
    let bounds = crate::default_window_bounds(cx);
    let mut options = crate::platform::window_options(bounds);
    // The workspace paints its own title bar. The bootstrap window instead
    // uses system decorations so close/minimize remain available before the
    // full workspace and its title-bar owner exist.
    options.window_decorations = Some(WindowDecorations::Server);
    if let Some(titlebar) = options.titlebar.as_mut() {
        titlebar.title = Some(SharedString::from("OxideTerm"));
        titlebar.appears_transparent = false;
        titlebar.traffic_light_position = None;
    }

    cx.open_window(options, |window, cx| {
        cx.new(|cx| {
            PortableBootstrapWindow::new(
                status,
                settings,
                PortableBootstrapLaunch {
                    native_connection_launch,
                    desktop_presence_menu,
                    single_instance_rx,
                    window_ui,
                },
                window,
                cx,
            )
        })
    })
    .map(|_| ())
}

impl PortableBootstrapWindow {
    fn new(
        status: PortableStatusSnapshot,
        settings: PersistedSettings,
        launch: PortableBootstrapLaunch,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        debug_assert!(portable_startup_requires_bootstrap(status.status));
        let focus_handle = cx.focus_handle();
        window.focus(&focus_handle, cx);
        Self {
            status,
            i18n: I18n::new(locale_from_settings(settings.general.language)),
            tokens: portable_bootstrap_tokens_from_settings(&settings),
            focus_handle,
            active_input: PortableBootstrapInput::Password,
            password: Zeroizing::new(String::new()),
            confirm_password: Zeroizing::new(String::new()),
            selection: 0..0,
            marked_range: None,
            password_bounds: None,
            confirm_password_bounds: None,
            pending_action: None,
            error: None,
            launch: Some(launch),
            action_task: None,
        }
    }

    fn is_setup(&self) -> bool {
        self.status.status == PortableBootstrapStatus::NeedsSetup
    }

    fn input_value(&self, input: PortableBootstrapInput) -> &str {
        match input {
            PortableBootstrapInput::Password => self.password.as_str(),
            PortableBootstrapInput::ConfirmPassword => self.confirm_password.as_str(),
        }
    }

    fn input_value_mut(&mut self, input: PortableBootstrapInput) -> &mut String {
        match input {
            PortableBootstrapInput::Password => &mut self.password,
            PortableBootstrapInput::ConfirmPassword => &mut self.confirm_password,
        }
    }

    fn focus_input(
        &mut self,
        input: PortableBootstrapInput,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.active_input = input;
        self.marked_range = None;
        let end = self.input_value(input).encode_utf16().count();
        self.selection = end..end;
        window.focus(&self.focus_handle, cx);
        cx.notify();
    }

    fn replace_active_text(
        &mut self,
        replacement_range: Option<Range<usize>>,
        text: &str,
        mark_inserted_text: bool,
        cx: &mut Context<Self>,
    ) {
        let value_len = self.input_value(self.active_input).encode_utf16().count();
        let range = replacement_range
            .or_else(|| self.marked_range.clone())
            .unwrap_or_else(|| self.selection.clone());
        let clamped = range.start.min(value_len)..range.end.min(value_len);
        let inserted_len = text.encode_utf16().count();
        replace_utf16(
            self.input_value_mut(self.active_input),
            clamped.clone(),
            text,
        );
        let inserted_range = clamped.start..clamped.start + inserted_len;
        self.selection = inserted_range.end..inserted_range.end;
        self.marked_range = mark_inserted_text.then_some(inserted_range);
        cx.notify();
    }

    fn delete_backward(&mut self, cx: &mut Context<Self>) {
        if self.selection.start != self.selection.end {
            self.replace_active_text(None, "", false, cx);
            return;
        }
        let caret = self.selection.start;
        if caret == 0 {
            return;
        }
        let value = self.input_value(self.active_input);
        let previous = previous_utf16_boundary(value, caret);
        self.replace_active_text(Some(previous..caret), "", false, cx);
    }

    fn paste_from_clipboard(&mut self, cx: &mut Context<Self>) {
        let Some(text) = cx.read_from_clipboard().and_then(|item| item.text()) else {
            return;
        };
        let clipboard_text = Zeroizing::new(text);
        // Browser password inputs are single-line. Keep that contract while
        // zeroizing both the clipboard copy and its normalized replacement.
        let normalized = Zeroizing::new(clipboard_text.replace(['\r', '\n'], ""));
        self.replace_active_text(None, normalized.as_str(), false, cx);
    }

    fn submit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.pending_action.is_some() || self.password.is_empty() {
            return;
        }
        if self.is_setup() && self.password.encode_utf16().count() < 6 {
            self.error = Some(self.i18n.t("portable_bootstrap.password_too_short"));
            cx.notify();
            return;
        }
        if self.is_setup() && self.password != self.confirm_password {
            self.error = Some(self.i18n.t("portable_bootstrap.password_mismatch"));
            cx.notify();
            return;
        }

        let password = std::mem::replace(&mut self.password, Zeroizing::new(String::new()));
        self.confirm_password.zeroize();
        self.selection = 0..0;
        self.marked_range = None;
        self.error = None;
        let action = if self.is_setup() {
            PortableBootstrapAction::Create
        } else {
            PortableBootstrapAction::Unlock
        };
        self.pending_action = Some(action);
        let window_handle = window.window_handle();

        // Argon2 deliberately consumes substantial CPU and memory. The secret
        // moves into one bounded background task and is zeroized when it exits.
        let keystore_task = cx.background_executor().spawn(async move {
            match action {
                PortableBootstrapAction::Create => {
                    oxideterm_portable_runtime::keystore::create_portable_keystore(
                        password.as_str(),
                    )
                }
                PortableBootstrapAction::Unlock => {
                    oxideterm_portable_runtime::keystore::unlock_portable_keystore(
                        password.as_str(),
                    )
                }
            }
            .map_err(|error| error.to_string())
        });
        self.action_task = Some(cx.spawn(async move |weak, cx| {
            let result = keystore_task.await;
            let _ = cx.update_window(window_handle, |_root, window, cx| {
                let _ = weak.update(cx, |this, cx| {
                    this.finish_action(result, window, cx);
                });
            });
        }));
        cx.notify();
    }

    fn finish_action(
        &mut self,
        result: Result<(), String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.action_task = None;
        self.pending_action = None;
        let Err(error) = result else {
            let Some(launch) = self.launch.take() else {
                self.error = Some("Portable startup state is no longer available".to_string());
                cx.notify();
                return;
            };
            match crate::open_main_workspace_window(
                cx,
                launch.native_connection_launch,
                launch.desktop_presence_menu,
                launch.single_instance_rx,
                launch.window_ui,
            ) {
                Ok(()) => {
                    #[cfg(target_os = "windows")]
                    if let Err(error) = crate::confirm_update_after_initial_workspace() {
                        eprintln!("failed to confirm the applied Windows update: {error}");
                    }
                    window.remove_window();
                }
                Err(error) => {
                    // The error contains startup structure only; secret drafts
                    // were moved and zeroized before workspace construction.
                    self.error = Some(error.to_string());
                    cx.notify();
                }
            }
            return;
        };

        self.error = Some(error);
        self.focus_input(PortableBootstrapInput::Password, window, cx);
    }

    fn handle_key_down(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.pending_action.is_some() {
            cx.stop_propagation();
            return;
        }

        let modifiers = event.keystroke.modifiers;
        let text_edit_modifier = if cfg!(target_os = "macos") {
            modifiers.platform
        } else {
            modifiers.platform || modifiers.control
        };
        if text_edit_modifier {
            match event.keystroke.key.as_str() {
                "a" => {
                    let end = self.input_value(self.active_input).encode_utf16().count();
                    self.selection = 0..end;
                    self.marked_range = None;
                    cx.stop_propagation();
                    cx.notify();
                }
                // Portable passwords must never leave the bootstrap owner via
                // an ordinary copy or cut action.
                "c" | "x" => cx.stop_propagation(),
                // The bound Paste action owns clipboard insertion. Stop the
                // following raw key event so the same text cannot be inserted twice.
                "v" => cx.stop_propagation(),
                _ => {}
            }
            return;
        }

        match event.keystroke.key.as_str() {
            "backspace" => {
                self.delete_backward(cx);
                cx.stop_propagation();
            }
            "tab" if self.is_setup() => {
                let next = if self.active_input == PortableBootstrapInput::Password {
                    PortableBootstrapInput::ConfirmPassword
                } else {
                    PortableBootstrapInput::Password
                };
                self.focus_input(next, window, cx);
                cx.stop_propagation();
            }
            "enter" => {
                self.submit(window, cx);
                cx.stop_propagation();
            }
            _ => {}
        }
    }

    fn render_password_input(
        &self,
        input: PortableBootstrapInput,
        label_key: &str,
        placeholder_key: &str,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let focused = self.active_input == input;
        let entity = cx.entity();
        let input_id = match input {
            PortableBootstrapInput::Password => 1,
            PortableBootstrapInput::ConfirmPassword => 2,
        };
        div()
            .w_full()
            .flex()
            .flex_col()
            .gap(px(8.0))
            .child(
                div()
                    .text_size(px(self.tokens.metrics.ui_text_sm))
                    .font_weight(gpui::FontWeight::MEDIUM)
                    .text_color(rgb(self.tokens.ui.text))
                    .child(self.i18n.t(label_key)),
            )
            .child(text_input_anchor_probe(
                oxideterm_gpui_ui::text_input::TextInputAnchorId(input_id),
                text_input(
                    &self.tokens,
                    TextInputView {
                        value: self.input_value(input),
                        placeholder: self.i18n.t(placeholder_key),
                        focused,
                        caret_visible: focused,
                        secret: true,
                        selected_all: false,
                        selected_range: focused.then(|| self.selection.clone()),
                        marked_text: None,
                    },
                )
                .w_full()
                .cursor(CursorStyle::IBeam)
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, _event, window, cx| {
                        this.focus_input(input, window, cx);
                        cx.stop_propagation();
                    }),
                ),
                move |anchor, _window, cx| {
                    let _ = entity.update(cx, |this, _cx| match input {
                        PortableBootstrapInput::Password => {
                            this.password_bounds = Some(anchor.bounds);
                        }
                        PortableBootstrapInput::ConfirmPassword => {
                            this.confirm_password_bounds = Some(anchor.bounds);
                        }
                    });
                },
            ))
    }

    fn render_path_row(&self, label_key: &str, value: String) -> impl IntoElement {
        div()
            .w_full()
            .flex()
            .flex_col()
            .gap(px(6.0))
            .child(
                div()
                    .text_size(px(self.tokens.metrics.ui_text_sm))
                    .text_color(rgb(self.tokens.ui.text_muted))
                    .child(self.i18n.t(label_key)),
            )
            .child(
                div()
                    .w_full()
                    .p(px(10.0))
                    .rounded(px(self.tokens.radii.sm))
                    .bg(rgb(self.tokens.ui.bg_sunken))
                    .text_size(px(self.tokens.metrics.ui_text_xs))
                    .text_color(rgb(self.tokens.ui.text_secondary))
                    .child(value),
            )
    }
}

impl Focusable for PortableBootstrapWindow {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for PortableBootstrapWindow {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        window.set_window_title(&SharedString::from(self.i18n.t("portable_bootstrap.title")));
        let setup = self.is_setup();
        let pending = self.pending_action.is_some();
        let submit_label = if pending {
            if setup {
                self.i18n.t("portable_bootstrap.setup_pending")
            } else {
                self.i18n.t("portable_bootstrap.unlock_pending")
            }
        } else if setup {
            self.i18n.t("portable_bootstrap.setup_submit")
        } else {
            self.i18n.t("portable_bootstrap.unlock_submit")
        };
        let submit = button(&self.tokens, submit_label, ButtonTone::Primary)
            .w_full()
            .opacity(if pending { 0.55 } else { 1.0 })
            .when(!pending, |button| {
                button.on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|this, _event, window, cx| {
                        this.submit(window, cx);
                        cx.stop_propagation();
                    }),
                )
            });

        let mut form =
            div()
                .w_full()
                .flex()
                .flex_col()
                .gap(px(16.0))
                .child(self.render_password_input(
                    PortableBootstrapInput::Password,
                    "portable_bootstrap.password_label",
                    "portable_bootstrap.password_placeholder",
                    cx,
                ));
        if setup {
            form = form.child(self.render_password_input(
                PortableBootstrapInput::ConfirmPassword,
                "portable_bootstrap.confirm_password_label",
                "portable_bootstrap.confirm_password_placeholder",
                cx,
            ));
        }
        if let Some(error) = self.error.clone() {
            form = form.child(
                div()
                    .w_full()
                    .p(px(10.0))
                    .rounded(px(self.tokens.radii.sm))
                    .border_1()
                    .border_color(rgba((self.tokens.ui.error << 8) | 0x66))
                    .bg(rgba((self.tokens.ui.error << 8) | 0x18))
                    .text_size(px(self.tokens.metrics.ui_text_sm))
                    .text_color(rgb(self.tokens.ui.error))
                    .child(error),
            );
        }
        form = form.child(submit);

        div()
            .id("portable-bootstrap-window")
            .track_focus(&self.focus_handle)
            .on_key_down(cx.listener(Self::handle_key_down))
            .on_action(cx.listener(|_this, _: &crate::Copy, _window, cx| {
                cx.stop_propagation();
            }))
            .on_action(cx.listener(|_this, _: &crate::Cut, _window, cx| {
                cx.stop_propagation();
            }))
            .on_action(cx.listener(|this, _: &crate::Paste, _window, cx| {
                this.paste_from_clipboard(cx);
                cx.stop_propagation();
            }))
            .size_full()
            .flex()
            .items_center()
            .justify_center()
            .p(px(32.0))
            .bg(rgb(self.tokens.ui.bg))
            .text_color(rgb(self.tokens.ui.text))
            .font_family(self.tokens.metrics.font_family)
            .child(
                div()
                    .w_full()
                    .max_w(px(760.0))
                    .flex()
                    .flex_col()
                    .gap(px(22.0))
                    .p(px(28.0))
                    .rounded(px(self.tokens.radii.lg))
                    .border_1()
                    .border_color(rgb(self.tokens.ui.border))
                    .bg(rgb(self.tokens.ui.bg_card))
                    .child(
                        div()
                            .text_size(px(self.tokens.metrics.ui_text_2xl))
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .text_color(rgb(self.tokens.ui.text_heading))
                            .child(self.i18n.t("portable_bootstrap.title")),
                    )
                    .child(
                        div()
                            .text_size(px(self.tokens.metrics.ui_text_base))
                            .line_height(px(self.tokens.metrics.ui_text_base + 8.0))
                            .text_color(rgb(self.tokens.ui.text_muted))
                            .child(if setup {
                                self.i18n.t("portable_bootstrap.setup_description")
                            } else {
                                self.i18n.t("portable_bootstrap.unlock_description")
                            }),
                    )
                    .child(
                        div()
                            .w_full()
                            .flex()
                            .flex_col()
                            .gap(px(12.0))
                            .child(self.render_path_row(
                                "portable_bootstrap.data_dir_label",
                                self.status.data_dir.clone(),
                            ))
                            .child(self.render_path_row(
                                "portable_bootstrap.keystore_path_label",
                                self.status.keystore_path.clone().unwrap_or_else(|| {
                                    self.i18n.t("portable_bootstrap.keystore_pending")
                                }),
                            )),
                    )
                    .child(form),
            )
            .child(PortableBootstrapInputElement {
                entity: cx.entity(),
                focus_handle: self.focus_handle.clone(),
            })
    }
}

struct PortableBootstrapInputElement {
    entity: gpui::Entity<PortableBootstrapWindow>,
    focus_handle: FocusHandle,
}

impl IntoElement for PortableBootstrapInputElement {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl Element for PortableBootstrapInputElement {
    type RequestLayoutState = ();
    type PrepaintState = ();

    fn id(&self) -> Option<ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static core::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, Self::RequestLayoutState) {
        let style = gpui::Style {
            size: gpui::Size {
                width: px(0.0).into(),
                height: px(0.0).into(),
            },
            ..Default::default()
        };
        (window.request_layout(style, None, cx), ())
    }

    fn prepaint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        _bounds: Bounds<Pixels>,
        _request_layout: &mut Self::RequestLayoutState,
        _window: &mut Window,
        _cx: &mut App,
    ) -> Self::PrepaintState {
    }

    fn paint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        _bounds: Bounds<Pixels>,
        _request_layout: &mut Self::RequestLayoutState,
        _prepaint: &mut Self::PrepaintState,
        window: &mut Window,
        cx: &mut App,
    ) {
        window.handle_input(
            &self.focus_handle,
            PortableBootstrapInputHandler {
                entity: self.entity.clone(),
            },
            cx,
        );
    }
}

struct PortableBootstrapInputHandler {
    entity: gpui::Entity<PortableBootstrapWindow>,
}

impl InputHandler for PortableBootstrapInputHandler {
    fn selected_text_range(
        &mut self,
        _ignore_disabled_input: bool,
        _window: &mut Window,
        cx: &mut App,
    ) -> Option<UTF16Selection> {
        Some(self.entity.update(cx, |this, _cx| UTF16Selection {
            range: this.selection.clone(),
            reversed: false,
        }))
    }

    fn marked_text_range(&mut self, _window: &mut Window, cx: &mut App) -> Option<Range<usize>> {
        self.entity
            .update(cx, |this, _cx| this.marked_range.clone())
    }

    fn text_for_range(
        &mut self,
        _range_utf16: Range<usize>,
        _adjusted_range: &mut Option<Range<usize>>,
        _window: &mut Window,
        _cx: &mut App,
    ) -> Option<String> {
        // Password controls accept replacement events but never return their
        // plaintext contents to the platform text service.
        None
    }

    fn replace_text_in_range(
        &mut self,
        replacement_range: Option<Range<usize>>,
        text: &str,
        _window: &mut Window,
        cx: &mut App,
    ) {
        self.entity.update(cx, |this, cx| {
            this.replace_active_text(replacement_range, text, false, cx);
        });
    }

    fn replace_and_mark_text_in_range(
        &mut self,
        range_utf16: Option<Range<usize>>,
        new_text: &str,
        _new_selected_range: Option<Range<usize>>,
        _window: &mut Window,
        cx: &mut App,
    ) {
        self.entity.update(cx, |this, cx| {
            this.replace_active_text(range_utf16, new_text, true, cx);
        });
    }

    fn unmark_text(&mut self, _window: &mut Window, cx: &mut App) {
        self.entity.update(cx, |this, cx| {
            this.marked_range = None;
            cx.notify();
        });
    }

    fn bounds_for_range(
        &mut self,
        _range_utf16: Range<usize>,
        _window: &mut Window,
        cx: &mut App,
    ) -> Option<Bounds<Pixels>> {
        self.entity.update(cx, |this, _cx| match this.active_input {
            PortableBootstrapInput::Password => this.password_bounds,
            PortableBootstrapInput::ConfirmPassword => this.confirm_password_bounds,
        })
    }

    fn character_index_for_point(
        &mut self,
        _point: gpui::Point<Pixels>,
        _window: &mut Window,
        cx: &mut App,
    ) -> Option<usize> {
        Some(self.entity.update(cx, |this, _cx| this.selection.end))
    }

    fn prefers_ime_for_printable_keys(&mut self, _window: &mut Window, _cx: &mut App) -> bool {
        true
    }
}

fn byte_index_for_utf16(value: &str, offset: usize) -> usize {
    let mut utf16_count = 0;
    for (byte_index, character) in value.char_indices() {
        if utf16_count >= offset {
            return byte_index;
        }
        utf16_count += character.len_utf16();
    }
    value.len()
}

fn replace_utf16(value: &mut String, range: Range<usize>, replacement: &str) {
    let start = byte_index_for_utf16(value, range.start);
    let end = byte_index_for_utf16(value, range.end);
    value.replace_range(start..end, replacement);
}

fn previous_utf16_boundary(value: &str, offset: usize) -> usize {
    let byte_index = byte_index_for_utf16(value, offset);
    value[..byte_index]
        .chars()
        .next_back()
        .map(|character| offset.saturating_sub(character.len_utf16()))
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn startup_gate_blocks_only_locked_portable_states() {
        assert!(!portable_startup_requires_bootstrap(
            PortableBootstrapStatus::Disabled
        ));
        assert!(portable_startup_requires_bootstrap(
            PortableBootstrapStatus::NeedsSetup
        ));
        assert!(portable_startup_requires_bootstrap(
            PortableBootstrapStatus::Locked
        ));
        assert!(!portable_startup_requires_bootstrap(
            PortableBootstrapStatus::Unlocked
        ));
    }

    #[test]
    fn utf16_edits_preserve_multibyte_password_text() {
        let mut value = "a🔐b".to_string();
        replace_utf16(&mut value, 1..3, "密");

        assert_eq!(value, "a密b");
        assert_eq!(previous_utf16_boundary(&value, 2), 1);
    }
}
