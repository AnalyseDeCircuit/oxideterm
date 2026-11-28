use std::{
    cell::RefCell,
    collections::{BTreeMap, BTreeSet, HashMap, hash_map::DefaultHasher},
    hash::{Hash, Hasher},
    rc::Rc,
};

use gpui::{
    AnyElement, App, Context, Div, EventEmitter, ListAlignment, ListState, MouseButton,
    MouseDownEvent, Render, Window, div, prelude::*, px, rgb, rgba, svg,
};
use oxideterm_ai::{
    AiProviderKeyDisplayState, AiProviderView,
    add_provider_from_template as ai_add_provider_from_template,
    add_provider_model as ai_add_provider_model, generated_provider_id,
    provider_key_display_state as ai_provider_key_display_state,
    provider_template_by_type as ai_provider_template_by_type, provider_view,
    set_active_provider_selection as ai_set_active_provider_selection,
};
use oxideterm_gpui_ui::{
    button::{
        ButtonOptions, ButtonRadius, ButtonSize, ButtonVariant, ToolbarButtonOptions,
        toolbar_button,
    },
    checkbox::checkbox,
    icons::LucideIcon,
    motion::animated_chevron,
    text_input::TextInputContentAlign,
    virtual_list::{
        TauriVirtualListSpec, VirtualListSignatureCache,
        sync_tauri_variable_list_state_by_signatures, tauri_virtual_list,
    },
};
use oxideterm_i18n::I18n;
use oxideterm_settings::PersistedSettings;
use oxideterm_settings_model::{
    AiProviderModelChipItem, ai_provider_card_signature, ai_provider_model_chip_rows,
    ai_update_provider, current_time_millis,
};
use oxideterm_theme::ThemeTokens;

use super::{available, confirm::AiSettingsConfirmIntent, section, settings_ai_collapsible_header};
use crate::{
    SettingsInput, SettingsSelect,
    components::{SettingsControlsSource, SettingsEdit},
};

mod add_controls;
mod card;
mod controls;
mod keys;
mod models;
mod section_list;

const AI_PROVIDER_SECTION_BORDER_ALPHA: u32 = 0xb3;
const AI_PROVIDER_MODEL_BORDER_ALPHA: u32 = 0x80;
const AI_PROVIDER_SELECT_W: f32 = 224.0;
const AI_PROVIDER_VISIBLE_MODEL_LIMIT: usize = 8;
const AI_PROVIDER_MODEL_CHIP_LIST_INITIAL_ROW_COUNT: usize = 0;
const AI_PROVIDER_MODEL_CHIPS_PER_VIRTUAL_ROW: usize = 4;
const AI_PROVIDER_MODEL_CHIP_ROW_ESTIMATED_HEIGHT: f32 = 28.0;
const AI_PROVIDER_MODEL_CHIP_ROW_OVERSCAN: usize = 6;
const AI_PROVIDER_CARD_LIST_ESTIMATED_HEIGHT: f32 = 220.0;
const AI_PROVIDER_CARD_LIST_OVERSCAN: usize = 3;

pub enum AiProviderRequest {
    Edit(SettingsEdit),
    RefreshModels(String),
    LoadKeyStatus(Vec<String>),
    SaveKey { index: usize, provider_id: String },
    Confirm(AiSettingsConfirmIntent, Option<String>),
    ClearModelDraft(SettingsInput),
}

pub fn is_provider_draft(input: SettingsInput) -> bool {
    matches!(
        input,
        SettingsInput::AiProviderApiKey(_) | SettingsInput::AiProviderNewModel(_)
    )
}

pub struct AiProviderBindings {
    pub controls: SettingsControlsSource,
    pub status: Rc<dyn Fn(&str, &App) -> (bool, bool)>,
    pub request: Rc<dyn Fn(AiProviderRequest, &mut App)>,
}

pub struct AiProviderSettingsView {
    bindings: AiProviderBindings,
    session_only: bool,
    tokens: ThemeTokens,
    i18n: I18n,
    enabled: bool,
    providers: Vec<Option<AiProviderView>>,
    active_provider_id: Option<String>,
    web_provider_id: Option<String>,
    expanded: bool,
    provider_type: String,
    expanded_providers: BTreeMap<String, bool>,
    expanded_models: BTreeSet<String>,
    provider_card_list_state: ListState,
    provider_card_list_cache: RefCell<VirtualListSignatureCache>,
    provider_model_chip_list_states: RefCell<HashMap<String, ListState>>,
    provider_model_chip_list_caches: RefCell<HashMap<String, VirtualListSignatureCache>>,
}

pub(crate) struct ProviderLayoutChanged;
impl EventEmitter<ProviderLayoutChanged> for AiProviderSettingsView {}

impl AiProviderSettingsView {
    pub(crate) fn for_session(bindings: AiProviderBindings) -> Self {
        Self {session_only: true, ..Self::new(bindings)}
    }
    pub fn new(bindings: AiProviderBindings) -> Self {
        Self {
            bindings,
            session_only: false,
            tokens: oxideterm_theme::default_tokens(),
            i18n: I18n::default(),
            enabled: false,
            providers: Vec::new(),
            active_provider_id: None,
            web_provider_id: None,
            expanded: true,
            provider_type: oxideterm_ai::AI_PROVIDER_TEMPLATES[0]
                .provider_type
                .to_owned(),
            expanded_providers: BTreeMap::new(),
            expanded_models: BTreeSet::new(),
            provider_card_list_state: ListState::new(
                0,
                ListAlignment::Top,
                TauriVirtualListSpec::new(
                    px(AI_PROVIDER_CARD_LIST_ESTIMATED_HEIGHT),
                    AI_PROVIDER_CARD_LIST_OVERSCAN,
                )
                .overdraw(),
            )
            .measure_all(),
            provider_card_list_cache: Default::default(),
            provider_model_chip_list_states: Default::default(),
            provider_model_chip_list_caches: Default::default(),
        }
    }

    pub fn provider_type(&self) -> &str {
        &self.provider_type
    }

    pub fn select_type(&mut self, provider_type: &str, cx: &mut Context<Self>) {
        self.provider_type = provider_type.to_owned();
        cx.notify();
    }

    pub fn expand_for_search(
        &mut self,
        ids: impl IntoIterator<Item = String>,
        cx: &mut Context<Self>,
    ) {
        self.expanded = true;
        for id in ids {
            self.expanded_providers.insert(id.clone(), true);
            self.expanded_models.insert(id);
        }
        cx.emit(ProviderLayoutChanged);
        cx.notify();
    }

    pub fn remove_provider(&mut self, id: &str, cx: &mut Context<Self>) {
        self.expanded_providers.remove(id);
        self.expanded_models.remove(id);
        self.provider_model_chip_list_states.get_mut().remove(id);
        self.provider_model_chip_list_caches.get_mut().remove(id);
        cx.emit(ProviderLayoutChanged);
        cx.notify();
    }

    pub fn hash_layout(&self, hasher: &mut impl Hasher) {
        self.expanded.hash(hasher);
        self.expanded_providers.hash(hasher);
        self.expanded_models.hash(hasher);
    }

    fn load_frame(&mut self, cx: &App) -> bool {
        let Some(controls) = (self.bindings.controls)(cx) else {
            return false;
        };
        self.tokens = *controls.tokens;
        self.i18n = controls.i18n.clone();
        let ai = &controls.settings.ai;
        self.enabled = ai.enabled;
        self.providers = ai.providers.iter().map(provider_view).collect();
        self.active_provider_id.clone_from(&ai.active_provider_id);
        self.web_provider_id = ai
            .web_search
            .enabled
            .then(|| ai.web_search.provider_id.clone())
            .flatten();
        true
    }

    fn provider(&self, index: usize) -> Option<&AiProviderView> {
        self.providers.get(index)?.as_ref()
    }

    fn settings_provider_expanded(&self, id: &str, default: bool) -> bool {
        self.expanded_providers.get(id).copied().unwrap_or(default)
    }

    fn toggle_settings_provider_expanded(
        &mut self,
        id: &str,
        default: bool,
        cx: &mut Context<Self>,
    ) {
        self.expanded_providers
            .insert(id.to_owned(), !self.settings_provider_expanded(id, default));
        cx.emit(ProviderLayoutChanged);
        cx.notify();
    }

    fn settings_provider_models_expanded(&self, id: &str) -> bool {
        self.expanded_models.contains(id)
    }

    fn toggle_settings_provider_models(&mut self, id: &str, cx: &mut Context<Self>) {
        if !self.expanded_models.remove(id) {
            self.expanded_models.insert(id.to_owned());
        }
        cx.emit(ProviderLayoutChanged);
        cx.notify();
    }

    fn edit_settings(&self, edit: impl FnOnce(&mut PersistedSettings) + 'static, cx: &mut App) {
        (self.bindings.request)(AiProviderRequest::Edit(Box::new(edit)), cx);
    }

    fn ai_provider_has_key_cached(&self, id: &str, cx: &App) -> bool {
        (self.bindings.status)(id, cx).0
    }

    fn is_web_provider(&self, provider: &AiProviderView) -> bool {
        self.web_provider_id.as_deref() == Some(provider.id.as_str())
            && oxideterm_ai::web::official_deepseek_provider(provider)
    }

    fn open_ai_settings_confirm(
        &self,
        intent: AiSettingsConfirmIntent,
        name: Option<String>,
        cx: &mut App,
    ) {
        (self.bindings.request)(AiProviderRequest::Confirm(intent, name), cx);
    }

    fn settings_panel_background(&self, color: u32, cx: &App) -> gpui::Rgba {
        (self.bindings.controls)(cx).map_or_else(
            || rgb(color).into(),
            |controls| controls.settings_panel_background(color),
        )
    }

    fn render_lucide_icon(icon: LucideIcon, size: f32, color: gpui::Rgba) -> AnyElement {
        svg()
            .path(icon.path())
            .size(px(size))
            .text_color(color)
            .into_any_element()
    }

    fn render_animated_chevron(
        &self,
        id: impl Into<gpui::ElementId>,
        expanded: bool,
        size: f32,
        color: gpui::Rgba,
    ) -> AnyElement {
        animated_chevron(
            &self.tokens,
            id,
            svg()
                .path(LucideIcon::ChevronRight.path())
                .size(px(size))
                .text_color(color),
            expanded,
        )
    }

    fn workspace_toolbar_action_button(
        &self,
        label: String,
        icon: Option<AnyElement>,
        options: ToolbarButtonOptions,
        action: impl Fn(&MouseDownEvent, &mut Window, &mut App) + 'static,
    ) -> Div {
        let enabled = !(options.button.disabled || options.loading);
        toolbar_button(&self.tokens, label, icon, options).on_mouse_down(
            MouseButton::Left,
            move |event, window, cx| {
                if enabled {
                    action(event, window, cx);
                } else {
                    cx.stop_propagation();
                }
            },
        )
    }

    fn i18n_count(&self, key: &str, count: usize) -> String {
        self.i18n.t(key).replace("{{count}}", &count.to_string())
    }

    fn field(
        &self,
        input: SettingsInput,
        value: &str,
        placeholder: String,
        secret: bool,
        cx: &App,
    ) -> AnyElement {
        (self.bindings.controls)(cx)
            .map(|controls| {
                controls.field(
                    input,
                    value,
                    placeholder,
                    None,
                    TextInputContentAlign::Start,
                    secret,
                    cx,
                )
            })
            .unwrap_or_else(|| div().into_any_element())
    }
}

impl Render for AiProviderSettingsView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if !self.load_frame(cx) {
            return div().into_any_element();
        }
        if self.enabled {
            let ids = self
                .providers
                .iter()
                .flatten()
                .filter(|provider| {
                    ai_provider_key_display_state(&provider.provider_type, false)
                        .shows_key_control()
                })
                .map(|provider| provider.id.clone())
                .collect();
            let request = self.bindings.request.clone();
            cx.defer(move |cx| request(AiProviderRequest::LoadKeyStatus(ids), cx));
        }
        available(self.ai_provider_settings_section(cx), self.enabled)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entity::SettingsWorkspaceEntity;

    #[gpui::test]
    fn provider_key_handoff_consumes_only_the_focused_secret(cx: &mut gpui::TestAppContext) {
        let state = cx.new(SettingsWorkspaceEntity::new);
        state.update(cx, |state, cx| {
            let input = SettingsInput::AiProviderApiKey(3);
            assert!(state.focus_settings_entity_input(input, cx));
            assert!(state.replace_settings_entity_input(input, None, "test-key", cx));
            let address = state.settings_entity_input_value(input).unwrap().as_ptr();
            assert!(
                state
                    .take_provider_key(SettingsInput::AiProviderApiKey(2), cx)
                    .is_none()
            );
            let secret = state.take_provider_key(input, cx).expect("focused secret");
            assert_eq!(secret.as_ptr(), address);
            assert_eq!(state.settings_entity_input_value(input), Some(""));
            assert!(state.take_provider_key(input, cx).is_none());
        });
    }
}
