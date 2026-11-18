use std::{collections::HashMap, rc::Rc};

use gpui::{
    AnyElement, App, Context, CursorStyle, Div, Entity, MouseButton, MouseDownEvent,
    MouseMoveEvent, Render, Rgba, Window, div, prelude::*, px, rgb, rgba, svg,
};
use oxideterm_ai::{
    AcpSessionConfigOption, AiProviderView, ModelSelectorProviderProbe, active_provider_view,
    model_selector_display_name, model_selector_truncated_label,
    model_selector_visible_provider_groups,
    provider_chat_requires_key as ai_provider_chat_requires_key,
    resolve_model_selector_provider_probe,
};
use oxideterm_gpui_ui::{
    ai::*,
    icons::LucideIcon,
    select::{
        BrowserFocusOrigin, OverlayAnchor, SelectAnchorId, browser_focus_visible,
        select_anchor_probe,
    },
    text_input::{TextInputAnchor, TextInputAnchorId, text_caret, text_input_anchor_probe},
};
use oxideterm_i18n::I18n;
use oxideterm_theme::ThemeTokens;

use crate::state::{
    AiAcpSessionState, AiModelSelectorScope, AiWorkspaceEntity, ai_acp_session_state,
};

#[derive(Default)]
pub struct AiModelPickerStatus {
    pub has_key: bool,
    pub online: bool,
    pub acp_fallback_label: Option<String>,
    pub acp_discovery_pending: bool,
    pub acp_options: Option<Vec<AcpSessionConfigOption>>,
}

pub struct AiModelPickerFrame {
    pub tokens: ThemeTokens,
    pub i18n: I18n,
    pub providers: Vec<AiProviderView>,
    pub status: HashMap<String, AiModelPickerStatus>,
    pub active_provider: Option<String>,
    pub active_model: Option<String>,
    pub acp_active: bool,
    pub active_acp_agent_id: Option<String>,
    pub marked: Option<String>,
    pub search_anchor: TextInputAnchorId,
    pub caret_visible: bool,
}

pub enum AiModelPickerRequest {
    Toggle(AiModelSelectorScope),
    Settings { close: bool },
    ManageAcp(bool),
    SearchPointer(MouseDownEvent),
    SearchDrag(MouseMoveEvent),
    Refresh(AiProviderView),
    Discover(String),
    Select(String, String),
    SelectAcpModel(String, String, String),
    SelectAcpConfig(String, String, String),
    SelectAcpMode(String, String),
}

#[derive(Clone)]
pub struct AiModelPickerBindings {
    pub frame: Rc<dyn Fn(&App) -> Option<AiModelPickerFrame>>,
    pub request: Rc<dyn Fn(AiModelPickerRequest, &mut Window, &mut App)>,
    pub anchor: Rc<dyn Fn(OverlayAnchor, &mut Window, &mut App)>,
    pub text_anchor: Rc<dyn Fn(TextInputAnchor, &mut Window, &mut App)>,
}

pub enum AiModelPickerPart {
    Trigger(AiModelSelectorScope, SelectAnchorId),
    Menu,
}

pub struct AiModelPickerView {
    state: Entity<AiWorkspaceEntity>,
    bindings: AiModelPickerBindings,
    part: AiModelPickerPart,
    frame: Option<AiModelPickerFrame>,
}

pub struct AiModelPickerViews {
    pub sidebar: Entity<AiModelPickerView>,
    pub inline: Entity<AiModelPickerView>,
    pub menu: Entity<AiModelPickerView>,
}

impl AiModelPickerView {
    pub fn new(
        state: Entity<AiWorkspaceEntity>,
        bindings: AiModelPickerBindings,
        part: AiModelPickerPart,
    ) -> Self {
        Self {
            state,
            bindings,
            part,
            frame: None,
        }
    }

    fn display(&self) -> &AiModelPickerFrame {
        self.frame
            .as_ref()
            .expect("model picker must have a display frame")
    }

    fn ai_model_selector_has_key(&self, provider: &AiProviderView, _: &App) -> bool {
        self.display()
            .status
            .get(&provider.id)
            .is_some_and(|status| status.has_key)
    }

    fn ai_model_selector_provider_is_online(&self, provider: &AiProviderView, _: &App) -> bool {
        self.display()
            .status
            .get(&provider.id)
            .is_some_and(|status| status.online)
    }

    fn ai_active_model_selector_provider_id(&self) -> Option<String> {
        self.display().active_provider.clone()
    }

    fn ai_acp_provider_id(agent_id: &str) -> String {
        format!("acp:{agent_id}")
    }

    fn ai_acp_agent_id_from_provider_id(provider_id: &str) -> Option<&str> {
        provider_id.strip_prefix("acp:")
    }

    fn active_ai_acp_session_state(&self, agent_id: &str, cx: &App) -> Option<AiAcpSessionState> {
        ai_acp_session_state(
            self.state
                .read(cx)
                .conversation_state()
                .active_conversation()?,
        )
        .filter(|state| state.agent_id == agent_id)
    }

    fn ai_acp_agent_model_fallback_label(&self, agent_id: &str, _: &App) -> String {
        self.display()
            .status
            .get(&Self::ai_acp_provider_id(agent_id))
            .and_then(|status| status.acp_fallback_label.clone())
            .unwrap_or_else(|| {
                self.display()
                    .i18n
                    .t("ai.model_selector.agent_model_unavailable")
            })
    }

    fn ai_acp_model_options_for_agent(
        &self,
        agent_id: &str,
        _: &App,
    ) -> Option<Vec<AcpSessionConfigOption>> {
        self.display()
            .status
            .get(&Self::ai_acp_provider_id(agent_id))
            .and_then(|status| status.acp_options.clone())
    }

    fn ai_acp_model_discovery_is_pending(&self, agent_id: &str, _: &App) -> bool {
        self.display()
            .status
            .get(&Self::ai_acp_provider_id(agent_id))
            .is_some_and(|status| status.acp_discovery_pending)
    }

    fn render_animated_chevron(
        &self,
        id: impl Into<gpui::ElementId>,
        expanded: bool,
        size: f32,
        color: Rgba,
    ) -> AnyElement {
        oxideterm_gpui_ui::motion::animated_chevron(
            &self.display().tokens,
            id,
            svg()
                .path(LucideIcon::ChevronRight.path())
                .size(px(size))
                .text_color(color),
            expanded,
        )
        .into_any_element()
    }
}

impl Render for AiModelPickerView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.frame = (self.bindings.frame)(cx);
        if self.frame.is_none() {
            return div().into_any_element();
        }
        match self.part {
            AiModelPickerPart::Trigger(scope, anchor) => {
                self.render_ai_model_selector(scope, anchor, cx)
            }
            AiModelPickerPart::Menu => {
                self.render_ai_model_selector_dropdown(&self.display().providers, cx)
            }
        }
    }
}

fn render_lucide_icon(icon: LucideIcon, size: f32, color: Rgba) -> AnyElement {
    svg()
        .path(icon.path())
        .size(px(size))
        .text_color(color)
        .into_any_element()
}

include!("model_picker/trigger.rs");
include!("model_picker/list.rs");
include!("model_picker/models.rs");
