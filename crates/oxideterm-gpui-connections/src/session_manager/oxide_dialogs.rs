// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use std::{
    cell::RefCell,
    collections::{HashMap, HashSet, hash_map::DefaultHasher},
    hash::{Hash, Hasher},
    rc::Rc,
    sync::Arc,
};

use chrono::Local;
use gpui::{
    AnyElement, App, Context, Div, Entity, ListAlignment, ListState, MouseButton, MouseDownEvent,
    Render, Rgba, ScrollEvent, ScrollHandle, Subscription, Window, div, prelude::*, px, relative,
    rgb, rgba,
};
use oxideterm_connections::{
    BatchDuplicateNamePolicy,
    oxide_file::{ForwardDetail, ImportConflictStrategy, ImportPreview},
};
use oxideterm_gpui_ui::{
    MaterialRole,
    button::{
        ButtonOptions, ButtonRadius, ButtonSize, ButtonVariant, IconButtonOptions,
        ToolbarButtonOptions,
    },
    checkbox,
    icons::LucideIcon,
    material_surface,
    modal::dismissible_dialog_backdrop,
    text_input::{TextInputView, text_input_anchor_probe, text_input_with_viewport},
    virtual_list::{
        TauriVirtualListSpec, sync_tauri_variable_list_state_by_signatures, tauri_virtual_list,
    },
};
use oxideterm_i18n::I18n;
use oxideterm_settings::ALL_OXIDE_SETTINGS_SECTIONS as OXIDE_APP_SETTINGS_SECTIONS;
use oxideterm_theme::ThemeTokens;

use super::*;
use crate::views::{ConnectionImeFrame, ConnectionsDispatch, ConnectionsWindowRequest as Request};

mod batch_view;

mod oxide_dialog_common;
#[allow(unused_imports)]
use oxide_dialog_common::*;
mod oxide_export_dialogs;
#[allow(unused_imports)]
use oxide_export_dialogs::*;
mod oxide_export_selection_dialogs;
#[allow(unused_imports)]
use oxide_export_selection_dialogs::*;
mod oxide_export_summary_dialogs;
#[allow(unused_imports)]
use oxide_export_summary_dialogs::*;
mod oxide_import_dialogs;
#[allow(unused_imports)]
use oxide_import_dialogs::*;
mod oxide_import_preview_dialogs;
#[allow(unused_imports)]
use oxide_import_preview_dialogs::*;
mod oxide_import_result_dialogs;
#[allow(unused_imports)]
use oxide_import_result_dialogs::*;

const OXIDE_MODAL_WIDTH: f32 = 672.0; // Tauri max-w-2xl
const OXIDE_MODAL_MAX_HEIGHT_RATIO: f32 = 0.85; // Tauri max-h-[85vh]
const OXIDE_MODAL_HEADER_PX: f32 = 24.0; // Tauri px-6
const OXIDE_MODAL_HEADER_PY: f32 = 16.0; // Tauri py-4
const OXIDE_MODAL_BODY_P: f32 = 24.0; // Tauri p-6
const OXIDE_MODAL_SECTION_GAP: f32 = 16.0; // Tauri space-y-4
const OXIDE_MODAL_CARD_P: f32 = 12.0; // Tauri p-3
const OXIDE_MODAL_LIST_MAX_H: f32 = 256.0; // Tauri max-h-64
const OXIDE_MODAL_FORWARDS_MAX_H: f32 = 208.0; // Tauri max-h-52
const OXIDE_SELECT_ALL_BUTTON_HEIGHT: f32 = 28.0; // Tauri OxideExportModal Button h-7
const OXIDE_BLUE_500: u32 = 0x3b82f6;
pub(super) const OXIDE_GREEN_500: u32 = 0x22c55e;
pub(super) const OXIDE_YELLOW_500: u32 = 0xeab308;
const OXIDE_RED_500: u32 = 0xef4444;
const OXIDE_ORANGE_500: u32 = 0xf97316;
const OXIDE_SLATE_400: u32 = 0x94a3b8;
const OXIDE_TONE_BG_ALPHA: u32 = 0x1a; // Tauri *-500/10
const OXIDE_TONE_BORDER_ALPHA: u32 = 0x33; // Tauri *-500/20
const OXIDE_SUBCARD_BG_ALPHA: u32 = 0x99; // Tauri bg-theme-bg-elevated/60 and bg-theme-bg/60
const OXIDE_NEW_BADGE_BG_ALPHA: u32 = 0x26; // Tauri bg-green-500/15

#[derive(Clone, Copy)]
pub enum ManagerTextRole {
    PlainDocument,
    RowSafe,
    NonSelectable,
}

use ManagerTextRole as SelectableTextRole;

pub enum OxideDialogAction {
    PickImportFile,
    PreviewImport,
    ApplyImport,
    RefreshExport,
    Export,
}

pub enum BatchWindowAction {
    Apply,
    PickDataFile,
    FocusEditor(BatchAddEditorKind),
    AuthKind(BatchAddAuthKind),
    DuplicatePolicy(BatchDuplicateNamePolicy),
    ChooseTemplate(String),
    NameTemplate,
    RenameTemplate(String),
    CommitTemplateName,
    DeleteTemplate(String),
    SaveTemplate,
    ChooseKey(String),
}

#[derive(Clone, Copy)]
pub(crate) enum ManagerOverlay {
    Oxide,
    Batch,
    BatchMenus,
}

#[derive(Default)]
pub struct OxideDialogEnvironment {
    pub saved_ids: HashSet<String>,
    pub serial_count: usize,
    pub telnet_count: usize,
    pub mosh_count: usize,
    pub remote_desktop_count: usize,
    pub ble_count: usize,
}

impl OxideDialogEnvironment {
    pub fn from_store(store: &oxideterm_connections::ConnectionStore) -> Self {
        Self {
            saved_ids: store.connections().iter().map(|record| record.id.clone())
                .chain(store.plugin_profiles().iter().map(|profile| profile.id.clone())).collect(),
            serial_count: store.serial_profiles().len(),
            telnet_count: store.telnet_profiles().len(),
            mosh_count: store.mosh_profiles().len(),
            remote_desktop_count: store.remote_desktop_profiles().len(),
            ble_count: store.ble_profiles().len(),
        }
    }
}

#[derive(Clone)]
pub struct SessionManagerViewBindings {
    pub catalog: Rc<dyn Fn(bool, &App) -> Option<CatalogEnvironment>>,
    pub oxide_environment: Rc<dyn Fn(&App) -> Option<OxideDialogEnvironment>>,
    pub ime: Rc<dyn Fn(SessionManagerInput, &App) -> Option<ConnectionImeFrame>>,
    pub text: Rc<dyn Fn(ManagerTextRole, &str, u64, String, u32, f32, &mut App) -> AnyElement>,
}

pub(crate) struct OxideDialogs {
    mount: ManagerOverlay,
    session_manager: Entity<SessionManagerState>,
    _observation: Subscription,
    tokens: ThemeTokens,
    i18n: I18n,
    theme: crate::views::ConnectionsEnvironmentSource,
    bindings: SessionManagerViewBindings,
    dispatch: ConnectionsDispatch,
    environment: OxideDialogEnvironment,
    scroll: RefCell<HashMap<&'static str, ScrollHandle>>,
    batch_anchors: Rc<
        RefCell<
            HashMap<
                oxideterm_gpui_ui::select::SelectAnchorId,
                oxideterm_gpui_ui::select::OverlayAnchor,
            >,
        >,
    >,
}

impl OxideDialogs {
    pub fn new(
        mount: ManagerOverlay,
        manager: Entity<SessionManagerState>,
        theme: crate::views::ConnectionsEnvironmentSource,
        bindings: SessionManagerViewBindings,
        dispatch: ConnectionsDispatch,
        cx: &mut Context<Self>,
        batch_anchors: Rc<
            RefCell<
                HashMap<
                    oxideterm_gpui_ui::select::SelectAnchorId,
                    oxideterm_gpui_ui::select::OverlayAnchor,
                >,
            >,
        >,
    ) -> Self {
        let observation = cx.observe(&manager, |_, _, cx| cx.notify());
        Self {
            mount,
            session_manager: manager,
            _observation: observation,
            tokens: oxideterm_theme::default_tokens(),
            i18n: I18n::new(oxideterm_i18n::Locale::En),
            theme,
            bindings,
            dispatch,
            environment: OxideDialogEnvironment::default(),
            scroll: Default::default(),
            batch_anchors,
        }
    }

    fn oxide_action(&self, action: OxideDialogAction, cx: &mut App) {
        (self.dispatch)(Request::Oxide(action), None, cx);
    }

    fn select_oxide_import_file(&self, cx: &mut App) {
        self.oxide_action(OxideDialogAction::PickImportFile, cx);
    }
    fn preview_oxide_import_dialog(&self, cx: &mut App) {
        self.oxide_action(OxideDialogAction::PreviewImport, cx);
    }
    fn apply_oxide_import_dialog(&self, cx: &mut App) {
        self.oxide_action(OxideDialogAction::ApplyImport, cx);
    }
    fn refresh_oxide_export_preflight(&self, cx: &mut App) {
        self.oxide_action(OxideDialogAction::RefreshExport, cx);
    }
    fn export_oxide_dialog(&self, cx: &mut App) {
        self.oxide_action(OxideDialogAction::Export, cx);
    }

    fn begin_oxide_import_dialog_exit(&self, cx: &mut App) -> bool {
        let delay = oxideterm_gpui_ui::motion::duration(
            &self.tokens,
            oxideterm_gpui_ui::motion::MotionDuration::Overlay,
        );
        self.session_manager.update(cx, |manager, cx| {
            manager.begin_import_dialog_exit(delay, cx)
        })
    }

    fn begin_oxide_export_dialog_exit(&self, cx: &mut App) -> bool {
        let delay = oxideterm_gpui_ui::motion::duration(
            &self.tokens,
            oxideterm_gpui_ui::motion::MotionDuration::Overlay,
        );
        self.session_manager.update(cx, |manager, cx| {
            manager.begin_export_dialog_exit(delay, cx)
        })
    }

    fn render_session_password_input(
        &self,
        target: SessionManagerInput,
        placeholder: String,
        cx: &App,
    ) -> AnyElement {
        let value = self
            .session_manager
            .read(cx)
            .input_value(target)
            .unwrap_or("");
        self.render_input(target, value, placeholder, true, cx)
    }

    fn render_session_text_input(
        &self,
        target: SessionManagerInput,
        value: &str,
        placeholder: String,
        cx: &App,
    ) -> AnyElement {
        self.render_input(target, value, placeholder, false, cx)
    }

    fn render_input(
        &self,
        target: SessionManagerInput,
        value: &str,
        placeholder: String,
        secret: bool,
        cx: &App,
    ) -> AnyElement {
        let Some(frame) = (self.bindings.ime)(target, cx) else {
            return div().into_any_element();
        };
        let focused = self.session_manager.read(cx).focused_input() == Some(target);
        let text = text_input_with_viewport(
            &self.tokens,
            TextInputView {
                value,
                placeholder,
                focused,
                caret_visible: frame.caret_visible,
                secret,
                selected_all: false,
                selected_range: frame.selected_range,
                marked_text: frame.marked_text.as_deref().map(String::as_str),
            },
            &frame.viewport,
            frame.active_offset,
        )
        .flex_1()
        .min_w_0()
        .h_full()
        .border_0()
        .bg(rgba(0x00000000))
        .px(px(0.0));
        let theme = self.tokens.ui;
        let focus = self.dispatch.clone();
        let drag = self.dispatch.clone();
        text_input_anchor_probe(
            frame.anchor_id,
            div()
                .id(("oxide-input", target.anchor_key()))
                .h(px(32.0))
                .w_full()
                .px_3()
                .flex()
                .items_center()
                .gap(px(8.0))
                .rounded(px(self.tokens.radii.md))
                .border_1()
                .border_color(if focused {
                    rgb(theme.accent)
                } else {
                    theme_border_half(theme.border, false)
                })
                .bg(theme_input_bg(theme.bg, false))
                .text_size(px(self.tokens.metrics.ui_text_sm))
                .child(text)
                .on_mouse_down(MouseButton::Left, move |event, window, cx| {
                    focus(
                        Request::FocusSessionInput(target, event.clone()),
                        Some(window),
                        cx,
                    );
                    cx.stop_propagation();
                })
                .on_mouse_move(move |event, window, cx| {
                    drag(Request::DragInputSelection(event.clone()), Some(window), cx);
                }),
            move |anchor, _, _| (frame.update_anchor)(anchor),
        )
        .into_any_element()
    }

    fn selectable_text_scroll_handle(&self, key: &'static str) -> ScrollHandle {
        self.scroll.borrow_mut().entry(key).or_default().clone()
    }

    fn render_display_text_with_role(
        &self,
        role: ManagerTextRole,
        scope: &str,
        key: impl Hash,
        value: impl Into<String>,
        color: u32,
        cx: &mut App,
    ) -> AnyElement {
        self.render_display_text_with_role_and_alpha(role, scope, key, value, color, 1.0, cx)
    }

    fn render_display_text_with_role_and_alpha(
        &self,
        role: ManagerTextRole,
        scope: &str,
        key: impl Hash,
        value: impl Into<String>,
        color: u32,
        alpha: f32,
        cx: &mut App,
    ) -> AnyElement {
        let mut hash = DefaultHasher::new();
        key.hash(&mut hash);
        (self.bindings.text)(role, scope, hash.finish(), value.into(), color, alpha, cx)
    }

    fn render_selectable_text_scoped(
        &self,
        scope: &str,
        key: impl Hash,
        value: impl Into<String>,
        color: u32,
        cx: &mut App,
    ) -> AnyElement {
        self.render_display_text_with_role(
            ManagerTextRole::PlainDocument,
            scope,
            key,
            value,
            color,
            cx,
        )
    }

    fn render_selectable_display_text(
        &self,
        scope: &str,
        key: impl Hash,
        value: impl Into<String>,
        color: u32,
        cx: &mut App,
    ) -> AnyElement {
        self.render_selectable_text_scoped(scope, key, value, color, cx)
    }

    fn workspace_toolbar_action_button(
        &self,
        label: String,
        icon: Option<AnyElement>,
        options: ToolbarButtonOptions,
        listener: impl Fn(&MouseDownEvent, &mut Window, &mut App) + 'static,
    ) -> gpui::Div {
        oxideterm_gpui_ui::button::toolbar_action_button(
            &self.tokens,
            label,
            icon,
            options,
            listener,
        )
    }

    fn workspace_modal_footer_action_button<T: Copy + Eq>(
        &self,
        label: String,
        variant: ButtonVariant,
        action: T,
        disabled: bool,
        focused: Option<T>,
        min_width: Option<f32>,
        listener: impl Fn(&mut Self, &MouseDownEvent, &mut Window, &mut Context<Self>) + 'static,
        cx: &mut Context<Self>,
    ) -> gpui::Div {
        self.workspace_toolbar_action_button(
            label,
            None,
            ToolbarButtonOptions {
                button: ButtonOptions {
                    variant,
                    size: ButtonSize::Sm,
                    radius: ButtonRadius::Md,
                    disabled,
                },
                min_width,
                focus_visible: focused == Some(action),
                ..Default::default()
            },
            cx.listener(listener),
        )
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
        let enabled = !(options.disabled || options.loading);
        oxideterm_gpui_ui::button::icon_button(
            &self.tokens,
            Self::render_lucide_icon(icon, size, color),
            options,
        )
        .when(enabled, |button| {
            button.on_mouse_down(MouseButton::Left, cx.listener(listener))
        })
        .when(!enabled, |button| {
            button.on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
        })
    }

    fn render_lucide_icon(icon: LucideIcon, size: f32, color: Rgba) -> AnyElement {
        gpui::svg()
            .path(icon.path())
            .size(px(size))
            .text_color(color)
            .into_any_element()
    }
}

impl Render for OxideDialogs {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let (import, export, batch) = {
            let state = self.session_manager.read(cx);
            (
                state.oxide_import_dialog.is_some(),
                state.oxide_export_dialog.is_some(),
                state.batch_add.is_some(),
            )
        };
        let visible = match self.mount {
            ManagerOverlay::Oxide => import || export,
            ManagerOverlay::Batch | ManagerOverlay::BatchMenus => batch,
        };
        if !visible {
            return div();
        }
        let (Some(theme), Some(environment)) =
            ((self.theme)(cx), (self.bindings.oxide_environment)(cx))
        else {
            return div();
        };
        self.tokens = theme.tokens;
        self.i18n = theme.i18n;
        self.environment = environment;
        let layer = div().absolute().inset_0();
        match self.mount {
            ManagerOverlay::Oxide => layer
                .children(import.then(|| self.render_oxide_import_dialog(cx)))
                .children(export.then(|| self.render_oxide_export_dialog(cx))),
            ManagerOverlay::Batch => layer.child(self.render_batch_add_dialog(cx)),
            ManagerOverlay::BatchMenus => layer
                .children(self.render_batch_add_template_overlay(window, cx))
                .children(self.render_batch_add_key_overlay(window, cx)),
        }
    }
}
