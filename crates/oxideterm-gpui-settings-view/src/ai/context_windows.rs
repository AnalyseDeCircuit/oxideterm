use std::{
    cell::RefCell,
    collections::{BTreeSet, HashMap},
    hash::{Hash, Hasher},
};

use gpui::{
    AnyElement, App, Entity, ListAlignment, ListState, MouseButton, div, prelude::*, px, rgb, svg,
};
use oxideterm_gpui_ui::{
    button::{ButtonOptions, ButtonSize, ButtonVariant, ToolbarButtonOptions, toolbar_button},
    icons::LucideIcon,
    motion::animated_chevron,
    virtual_list::{
        TauriVirtualListSpec, VirtualListSignatureCache,
        sync_tauri_variable_list_state_by_signatures, tauri_virtual_list,
    },
};
use oxideterm_settings_model::{
    AiProviderModelPanel, ai_model_context_window_panels, ai_model_context_window_row,
    ai_provider_model_row_signature, ai_provider_views, persisted_settings_input_value,
    set_ai_user_context_window,
};

use super::*;
use crate::{
    SettingsInput,
    components::{
        SettingsControls, SettingsControlsSource, SettingsWindowRequest, settings_mono_font_family,
    },
    entity::SettingsWorkspaceEntity,
};

const ROW_HEIGHT: f32 = 48.0;
const ROW_OVERSCAN: usize = 6;

#[derive(Default)]
pub struct AiContextWindowState {
    pub expanded: bool,
    pub expanded_providers: BTreeSet<String>,
    lists: RefCell<HashMap<String, (ListState, VirtualListSignatureCache)>>,
}

impl AiContextWindowState {
    pub fn remove_provider(&mut self, provider: &str) {
        self.expanded_providers.remove(provider);
        self.lists.get_mut().remove(provider);
    }

    pub fn hash_layout(&self, hasher: &mut impl Hasher) {
        self.expanded.hash(hasher);
        self.expanded_providers.hash(hasher);
    }
}

pub(crate) fn render(
    controls: &SettingsControls<'_>,
    owner: &Entity<SettingsWorkspaceEntity>,
    source: SettingsControlsSource,
    cx: &App,
) -> AnyElement {
    let settings = controls.settings;
    let expanded = owner.read(cx).ai_context_windows.expanded;
    let header = settings_ai_context_windows_header(
        controls.tokens,
        controls.i18n.t("settings_view.ai.model_context_windows"),
        controls
            .i18n
            .t("settings_view.ai.model_context_windows_hint"),
        chevron(
            controls.tokens,
            ("ai-context-windows-chevron", expanded as usize),
            expanded,
            16.0,
        ),
        768.0,
    )
    .on_mouse_down(MouseButton::Left, {
        let owner = owner.downgrade();
        move |_, _, cx| {
            let _ = owner.update(cx, |state, cx| {
                state.ai_context_windows.expanded = !state.ai_context_windows.expanded;
                cx.notify();
            });
            cx.stop_propagation();
        }
    })
    .into_any_element();
    let body = expanded.then(|| {
        let mut rows = ai_model_context_window_panels(settings, &ai_provider_views(settings))
            .into_iter()
            .map(|data| provider(controls, owner, source.clone(), data, cx))
            .collect::<Vec<_>>();
        if rows.is_empty() {
            rows.push(settings_ai_model_empty_text(
                controls.tokens,
                controls
                    .i18n
                    .t("settings_view.ai.model_context_windows_empty"),
            ));
        }
        controls.plain_settings_card(rows)
    });
    available(
        super::section(controls.tokens, header, body),
        settings.ai.enabled,
    )
}

fn chevron(
    tokens: &ThemeTokens,
    id: impl Into<gpui::ElementId>,
    expanded: bool,
    size: f32,
) -> AnyElement {
    animated_chevron(
        tokens,
        id,
        svg()
            .path(LucideIcon::ChevronRight.path())
            .size(px(size))
            .text_color(rgb(tokens.ui.text_muted)),
        expanded,
    )
}

fn provider(
    controls: &SettingsControls<'_>,
    owner: &Entity<SettingsWorkspaceEntity>,
    source: SettingsControlsSource,
    data: AiProviderModelPanel,
    cx: &App,
) -> AnyElement {
    let AiProviderModelPanel {
        provider_id,
        provider_name,
        provider_index,
        model_count,
        override_count,
        models,
        ..
    } = data;
    let state = &owner.read(cx).ai_context_windows;
    let expanded = state.expanded_providers.contains(&provider_id);
    let header = settings_ai_model_provider_header(
        controls.tokens,
        provider_name,
        controls
            .i18n
            .t("settings_view.ai.ctx_provider_summary")
            .replace("{{count}}", &model_count.to_string())
            .replace("{{overrides}}", &override_count.to_string()),
        chevron(
            controls.tokens,
            (
                SharedString::from(format!("ai-context-provider-chevron-{provider_id}")),
                expanded as usize,
            ),
            expanded,
            14.0,
        ),
    )
    .on_mouse_down(MouseButton::Left, {
        let owner = owner.downgrade();
        let provider_id = provider_id.clone();
        move |_, _, cx| {
            let _ = owner.update(cx, |state, cx| {
                let providers = &mut state.ai_context_windows.expanded_providers;
                if !providers.remove(&provider_id) {
                    providers.insert(provider_id.clone());
                }
                cx.notify();
            });
            cx.stop_propagation();
        }
    });
    let rows = expanded.then(|| {
        let spec = TauriVirtualListSpec::new(px(ROW_HEIGHT), ROW_OVERSCAN);
        let overrides = controls.settings.ai.user_context_windows.get(&provider_id);
        let signatures = models
            .iter()
            .map(|model| {
                ai_provider_model_row_signature(
                    &provider_id,
                    model,
                    overrides.and_then(|entries| entries.get(model)),
                )
            })
            .collect::<Vec<_>>();
        let list = {
            let mut lists = state.lists.borrow_mut();
            let (list, cache) = lists.entry(provider_id.clone()).or_insert_with(|| {
                (
                    ListState::new(0, ListAlignment::Top, spec.overdraw()).measure_all(),
                    VirtualListSignatureCache::default(),
                )
            });
            sync_tauri_variable_list_state_by_signatures(
                list,
                cache,
                &format!("ai-context-models:{provider_id}"),
                &signatures,
                spec,
            );
            list.clone()
        };
        settings_ai_model_row_list_frame(
            controls.tokens,
            models.len() as f32 * ROW_HEIGHT,
            tauri_virtual_list(list, spec, move |model_index, _, cx| {
                let Some((controls, model)) = source(cx).zip(models.get(model_index)) else {
                    return div().into_any_element();
                };
                model_row(
                    &controls,
                    provider_index,
                    &provider_id,
                    model_index,
                    model,
                    cx,
                )
            })
            .into_any_element(),
        )
    });
    settings_ai_model_provider_section(header, rows)
}

fn model_row(
    controls: &SettingsControls<'_>,
    provider_index: usize,
    provider_id: &str,
    model_index: usize,
    model: &str,
    cx: &App,
) -> AnyElement {
    let settings = controls.settings;
    let input = SettingsInput::AiModelContextWindow(provider_index, model_index);
    let row = ai_model_context_window_row(settings, provider_id, model);
    let reset = toolbar_button(
        controls.tokens,
        controls.i18n.t("settings_view.ai.ctx_reset"),
        None,
        ToolbarButtonOptions {
            button: ButtonOptions {
                variant: ButtonVariant::Ghost,
                size: ButtonSize::Sm,
                disabled: !row.has_override,
                ..Default::default()
            },
            height: Some(28.0),
            ..Default::default()
        },
    )
    .on_mouse_down(MouseButton::Left, {
        let dispatch = controls.dispatch.clone();
        let provider_id = provider_id.to_owned();
        let model = model.to_owned();
        move |_, window, cx| {
            if row.has_override {
                let provider_id = provider_id.clone();
                let model = model.clone();
                dispatch(
                    SettingsWindowRequest::Edit(Box::new(move |settings| {
                        set_ai_user_context_window(settings, &provider_id, &model, None);
                    })),
                    window,
                    cx,
                );
            }
            cx.stop_propagation();
        }
    })
    .into_any_element();
    settings_ai_context_window_row(
        controls.tokens,
        settings_mono_font_family(settings),
        model.to_owned(),
        settings_ai_context_source_badge_for_source(
            controls.tokens,
            controls.i18n.t(row.source.i18n_key()),
            row.source,
        ),
        controls.settings_text_input_control(
            input,
            persisted_settings_input_value(settings, input).unwrap_or_default(),
            controls.i18n.t("settings_view.ai.automatic_placeholder"),
            112.0,
            cx,
        ),
        Some(reset),
        row.has_override,
        model_index == 0,
    )
}
