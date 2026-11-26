use std::{
    cell::RefCell,
    collections::{HashMap, HashSet},
    hash::{Hash, Hasher},
    rc::Rc,
    sync::Arc,
};

use crate::state::{
    AiChatFooterAction, AiChatPopover, AiCompactionNoticePhase, AiContextTokenBreakdown,
    ai_acp_session_state,
};
use base64::Engine as _;
use gpui::{
    AnyElement, App, Context, Div, Entity, MouseButton, MouseDownEvent, Render, Rgba, ScrollHandle,
    SharedString, StyledText, TextRun, Window, div, font, prelude::*, px, rgb, rgba, svg,
};
use oxideterm_ai::{
    AcpAuthMethod, AiReasoningLevel,
    stream_state::{AI_CONTEXT_DANGER_PERCENT, ai_context_percentage},
};
use oxideterm_ai::{AiAutocompleteCandidate, AiAutocompleteKind, AiProviderView};
use oxideterm_ai::{
    AiChatMessage, AiChatRole, AiImageAttachment,
    agent::{AgentModel, AgentState, AgentText},
    ai_visible_suggestion_content,
};
use oxideterm_gpui_markdown::{
    MarkdownBlockLayout, MarkdownDocument, MarkdownOptions, parser as markdown_parser,
    render as markdown_render,
    render::{MarkdownCodeBlockActions, MarkdownTextLink},
};
use oxideterm_gpui_ui::select::{OverlayAnchor, SelectAnchorId, select_anchor_probe};
use oxideterm_gpui_ui::{
    ConfirmDialogAction, ConfirmDialogVariant, ConfirmDialogView,
    text_input::{
        TextInputAnchor, TextInputAnchorId, TextInputContentAlign, TextInputView,
        TextInputViewport, text_input_anchor_probe, text_input_with_viewport_and_content_align,
    },
};
use oxideterm_gpui_ui::{
    MaterialRole,
    ai::*,
    button::{
        ButtonOptions, ButtonRadius, ButtonSize, ButtonVariant, IconButtonOptions,
        ToolbarButtonOptions,
    },
    checkbox,
    disclosure_motion::DisclosureMotions,
    icons::LucideIcon,
    material_surface,
    virtual_list::{TauriVirtualListSpec, tauri_virtual_list},
};
use oxideterm_i18n::I18n;
use oxideterm_settings::{AiActiveBackend, AiThinkingStyle};
use oxideterm_theme::ThemeTokens;

use super::editor::AiEditorView;
use crate::state::{AiChatInitializationError, AiWorkspaceEntity, ai_chat_virtual_list_spec};

#[derive(Clone, Copy, Eq, PartialEq)]
pub enum SelectableTextRole {
    PlainDocument,
    RowSafe,
    NonSelectable,
}

pub struct AiTextFragment {
    pub role: SelectableTextRole,
    pub group: u64,
    pub id: u64,
    pub order: usize,
    pub text: SharedString,
    pub runs: Vec<TextRun>,
    pub links: Vec<MarkdownTextLink>,
    pub join_previous: bool,
}

type TextRenderer = Rc<dyn Fn(AiTextFragment) -> AnyElement>;

pub struct AiComposerFrame {
    pub enabled: bool,
    pub model_selected: bool,
    pub images_allowed: bool,
    pub backend: AiActiveBackend,
    pub context_counts: Option<(usize, usize)>,
    pub terminal_context: bool,
    pub split_context: bool,
    pub ide_context: bool,
    pub sftp_context: bool,
    pub tools: (bool, usize),
    pub reasoning: Option<AiReasoningLevel>,
    pub auth_methods: Option<Vec<AcpAuthMethod>>,
}

pub struct AiTranscriptPolicy {
    pub model_selected: bool,
    pub images_allowed: bool,
    pub reasoning: Option<(AiProviderView, String, AiReasoningLevel)>,
    pub agent_models: Vec<AgentModel>,
}

impl AiTranscriptPolicy {
    pub fn from_settings(
        ai: &oxideterm_settings::AiSettings,
        state: &AiWorkspaceEntity,
        stream_config: impl FnOnce() -> Option<oxideterm_ai::AiChatStreamConfig>,
    ) -> Self {
        use oxideterm_ai::{active_model_selection, active_provider_view};
        let providers = oxideterm_ai::provider_views(&ai.providers);
        let acp = ai.active_backend == AiActiveBackend::Acp;
        let selected = active_model_selection(ai.active_model.as_deref());
        let reasoning = (!acp)
            .then(|| {
                let provider = active_provider_view(&providers, ai.active_provider_id.as_deref())?;
                let model = selected.as_ref()?;
                if oxideterm_ai::model_reasoning_capability(&provider.provider_type, model)
                    .levels
                    .is_empty()
                {
                    return None;
                }
                let override_value = state
                    .conversation_state()
                    .active_conversation()
                    .and_then(|conversation| {
                        crate::state::ai_conversation_reasoning_effort(
                            conversation,
                            &provider.id,
                            model,
                        )
                    })
                    .or_else(|| {
                        ai.reasoning_model_overrides
                            .get(&provider.id)
                            .and_then(|models| models.get(model))
                            .and_then(serde_json::Value::as_str)
                    })
                    .unwrap_or("auto");
                let level = oxideterm_ai::normalize_reasoning_level_for_model(
                    &provider.provider_type,
                    model,
                    override_value,
                );
                Some((provider.clone(), model.clone(), level))
            })
            .flatten();
        let model_selected = if acp {
            ai.active_acp_agent_id.is_some()
        } else {
            ai.active_provider_id.is_some() && selected.is_some()
        };
        let images_allowed = state.chat_ui().images.is_empty()
            || stream_config().is_some_and(|config| {
                config.execution_backend == oxideterm_ai::AiExecutionBackend::Acp
                    || oxideterm_ai::model_image_input(
                        &config.provider_type,
                        &config.model,
                        config.image_input,
                    ) != Some(false)
            });
        let agent_models = providers
            .into_iter()
            .filter(|provider| provider.enabled)
            .flat_map(|provider| {
                provider.models.into_iter().map(move |model| AgentModel {
                    provider_id: provider.id.clone(),
                    model,
                })
            })
            .collect();
        Self {
            model_selected,
            images_allowed,
            reasoning,
            agent_models,
        }
    }
}

pub struct AiSidebarChildren {
    pub header: Entity<super::AiChatHeaderView>,
    pub background: Entity<crate::background::AiBackgroundTaskView>,
    pub model_trigger: Entity<crate::model_picker::AiModelPickerView>,
    pub composer: Entity<AiEditorView>,
}

pub struct AiRenameFrame {
    pub anchor: TextInputAnchorId,
    pub viewport: TextInputViewport,
    pub marked: Option<String>,
    pub selected_range: Option<std::ops::Range<usize>>,
    pub active_offset: Option<usize>,
    pub caret_visible: bool,
}

pub struct AiOverlayFrame {
    pub rename: AiRenameFrame,
    pub reasoning: Option<(AiProviderView, String, AiReasoningLevel)>,
    pub breakdown: Option<AiContextTokenBreakdown>,
    pub confirm_focus: Option<ConfirmDialogAction>,
}

pub struct AiTranscriptFrame {
    pub overlay: AiOverlayFrame,

    pub composer: AiComposerFrame,

    pub tokens: ThemeTokens,
    pub i18n: I18n,
    pub font: SharedString,
    pub mono_font: SharedString,
    pub panel_background: Rgba,
    pub divider: Rgba,
    pub window_background: bool,
    pub thinking_style: AiThinkingStyle,
    pub thinking_default_expanded: bool,
    pub agent_models: Vec<AgentModel>,
    pub markdown: MarkdownOptions,
    pub selectable_text: TextRenderer,
}

pub enum AiTranscriptRequest {
    ClipboardReadFailed,
    Regenerate,
    CancelEdit,
    SaveEdit,
    FollowUp(String),
    AcpPermission(u64, String, Option<String>),
    InsertCommand(String),
    ImagePreviewFailed,
    AgentModelLocked,
    BeginRename(String, String),
    RenamePointer(MouseDownEvent),
    RenameDrag(gpui::MouseMoveEvent),
    SaveRename,
    DeleteConversation(String),
    SafetyDefault,
    SafetyReadOnly,
    SafetyAuto,
    SafetyConfirm,
    ConfirmSafety(bool),
    ConfirmSummary(bool),
    ConfirmClearAll(bool),
    ConfirmDeleteMessage(bool),
    Autocomplete(AiAutocompleteCandidate),
    SelectReasoning(String, String, String, AiReasoningLevel),

    OpenSettings,
    ClosePopovers,
    NewConversation,
    Stop,
    Steer,
    InterruptAndSend,
    Send,
    Compact,
    Summarize,
    SelectImages,
    ContinueTextOnly(String),
    ImportImages(Vec<std::path::PathBuf>),
    Authenticate(String, String),
    QueueAction(String, String, &'static str),
    EditMessage(String),
    FocusComposer,
}

#[cfg(not(target_family = "wasm"))]
pub struct AiTranscriptBindings {
    pub history_path: std::path::PathBuf,
    pub runtime: Arc<tokio::runtime::Runtime>,
    pub frame: Rc<dyn Fn(&App) -> Option<AiTranscriptFrame>>,
    pub request: Rc<dyn Fn(AiTranscriptRequest, Option<&mut Window>, &mut App)>,
    pub file_preview: Rc<dyn Fn(&str, &App) -> Option<String>>,
    pub markdown_actions: MarkdownCodeBlockActions,
    pub select_anchor: Rc<dyn Fn(OverlayAnchor, &mut Window, &mut App)>,
    pub text_anchor: Rc<dyn Fn(TextInputAnchor, &mut Window, &mut App)>,
}

pub struct PortableTranscriptBindings {
    pub initialize: Rc<dyn Fn(&mut AiWorkspaceEntity, bool, &mut Context<AiWorkspaceEntity>)>,
    pub image_bytes: Rc<
        dyn Fn(
            &AiWorkspaceEntity,
            Option<String>,
            AiImageAttachment,
        ) -> std::pin::Pin<Box<dyn Future<Output = Option<Vec<u8>>>>>,
    >,
    pub frame: Rc<dyn Fn(&App) -> Option<AiTranscriptFrame>>,
    pub request: Rc<dyn Fn(AiTranscriptRequest, Option<&mut Window>, &mut App)>,
    pub file_preview: Rc<dyn Fn(&str, &App) -> Option<String>>,
    pub markdown_actions: MarkdownCodeBlockActions,
    pub select_anchor: Rc<dyn Fn(OverlayAnchor, &mut Window, &mut App)>,
    pub text_anchor: Rc<dyn Fn(TextInputAnchor, &mut Window, &mut App)>,
}

#[cfg(not(target_family = "wasm"))]
impl From<AiTranscriptBindings> for PortableTranscriptBindings {
    fn from(bindings: AiTranscriptBindings) -> Self {
        Self {
            initialize: Rc::new(move |state, retry, cx| {
                if retry {
                    state.retry_chat_initialization(bindings.history_path.clone(), cx);
                } else {
                    state.ensure_chat_initialized(bindings.history_path.clone(), cx);
                }
            }),
            image_bytes: Rc::new(move |state, conversation, image| {
                let store = state.history.store.clone();
                let work = bindings.runtime.spawn_blocking(move || match image.data {
                    Some(data) => base64::engine::general_purpose::STANDARD
                        .decode(data.as_bytes())
                        .ok(),
                    None => Some(
                        store?
                            .image_bytes(conversation.as_deref()?, &image.id)
                            .ok()?
                            .to_vec(),
                    ),
                });
                Box::pin(async move { work.await.ok().flatten() })
            }),
            frame: bindings.frame,
            request: bindings.request,
            file_preview: bindings.file_preview,
            markdown_actions: bindings.markdown_actions,
            select_anchor: bindings.select_anchor,
            text_anchor: bindings.text_anchor,
        }
    }
}

pub struct AiAssistantSidebarView {
    state: Entity<AiWorkspaceEntity>,
    message_editor: Entity<AiEditorView>,
    views: AiSidebarChildren,
    bindings: PortableTranscriptBindings,
    frame: Option<AiTranscriptFrame>,
    tokens: ThemeTokens,
    i18n: I18n,
    disclosure_motions: DisclosureMotions,
    scroll_handles: RefCell<HashMap<String, ScrollHandle>>,
}

impl AiAssistantSidebarView {
    #[cfg(not(target_family = "wasm"))]
    pub fn new(
        state: Entity<AiWorkspaceEntity>,
        message_editor: Entity<AiEditorView>,
        views: AiSidebarChildren,
        bindings: AiTranscriptBindings,
        tokens: ThemeTokens,
        i18n: I18n,
        cx: &mut Context<Self>,
    ) -> Self {
        Self::with_bindings(
            state,
            message_editor,
            views,
            bindings.into(),
            tokens,
            i18n,
            cx,
        )
    }

    pub fn with_bindings(
        state: Entity<AiWorkspaceEntity>,
        message_editor: Entity<AiEditorView>,
        views: AiSidebarChildren,
        bindings: PortableTranscriptBindings,
        tokens: ThemeTokens,
        i18n: I18n,
        cx: &mut Context<Self>,
    ) -> Self {
        cx.observe(&state, |_, _, cx| cx.notify()).detach();
        Self {
            state,
            message_editor,
            views,
            bindings,
            frame: None,
            tokens,
            i18n,
            disclosure_motions: DisclosureMotions::default(),
            scroll_handles: RefCell::new(HashMap::new()),
        }
    }

    fn close_popovers(&self, cx: &mut App) {
        (self.bindings.request)(AiTranscriptRequest::ClosePopovers, None, cx);
    }

    fn place_select_anchor(&self) -> impl FnOnce(OverlayAnchor, &mut Window, &mut App) + use<> {
        let place = self.bindings.select_anchor.clone();
        move |anchor, window, cx| place(anchor, window, cx)
    }

    fn active_ai_safety_mode(&self, cx: &App) -> AiSafetyMode {
        self.state.read(cx).active_conversation_safety_mode()
    }

    fn refresh_frame(&mut self, cx: &App) -> bool {
        let Some(frame) = (self.bindings.frame)(cx) else {
            return false;
        };
        self.tokens = frame.tokens;
        self.i18n = frame.i18n.clone();
        if !self.tokens.motion.enabled {
            self.disclosure_motions.clear();
        }
        self.frame = Some(frame);
        true
    }

    fn ai_conversation_turns_label(&self, count: usize) -> String {
        self.i18n
            .t("ai.chat.turns_count")
            .replace("{{count}}", &count.to_string())
    }

    fn frame(&self) -> &AiTranscriptFrame {
        self.frame.as_ref().expect("AI transcript frame")
    }

    fn context_sidebar_content_background(&self, _: u32) -> Rgba {
        self.frame().panel_background
    }

    fn workspace_chrome_divider(&self) -> Rgba {
        self.frame().divider
    }

    fn selectable_text_scroll_handle(&self, key: impl Into<String>) -> ScrollHandle {
        self.scroll_handles
            .borrow_mut()
            .entry(key.into())
            .or_default()
            .clone()
    }

    fn localized_markdown_options(&self) -> MarkdownOptions {
        self.frame().markdown.clone()
    }

    fn markdown_mermaid_actions(&self, _: &App) -> MarkdownCodeBlockActions {
        self.bindings.markdown_actions.clone()
    }

    fn selectable_text_render_state(&self, _: &App) -> AiTextView {
        AiTextView {
            render: self.frame().selectable_text.clone(),
            font: self.frame().font.clone(),
        }
    }

    fn render_display_text_with_role(
        &self,
        role: SelectableTextRole,
        scope: &str,
        key: impl Hash,
        text: impl Into<String>,
        color: u32,
        cx: &App,
    ) -> AnyElement {
        self.render_display_text_with_role_and_alpha(role, scope, key, text, color, 1.0, cx)
    }

    fn render_display_text_with_role_and_alpha(
        &self,
        role: SelectableTextRole,
        scope: &str,
        key: impl Hash,
        text: impl Into<String>,
        color: u32,
        alpha: f32,
        cx: &App,
    ) -> AnyElement {
        let text = text.into();
        let id = selectable_text_id(scope, (key, text.as_str()));
        let view = self.selectable_text_render_state(cx);
        let run = view.run(
            &text,
            rgba((color << 8) | (alpha.clamp(0.0, 1.0) * 255.0).round() as u32),
        );
        view.render_styled_text_in_group(
            role,
            selectable_document_group_id(),
            id,
            0,
            text.into(),
            vec![run],
            Vec::new(),
            false,
        )
    }

    fn render_selectable_text(
        &self,
        id: u64,
        text: impl Into<SharedString>,
        color: u32,
        cx: &App,
    ) -> AnyElement {
        self.render_selectable_text_in_group(selectable_document_group_id(), id, 0, text, color, cx)
    }

    fn render_selectable_text_scoped(
        &self,
        scope: &str,
        key: impl Hash,
        text: impl Into<SharedString>,
        color: u32,
        cx: &App,
    ) -> AnyElement {
        let text = text.into();
        let id = selectable_text_id(scope, (key, text.as_ref()));
        self.render_selectable_text(id, text, color, cx)
    }

    fn render_selectable_text_in_group(
        &self,
        group: u64,
        id: u64,
        order: usize,
        text: impl Into<SharedString>,
        color: u32,
        cx: &App,
    ) -> AnyElement {
        let text = text.into();
        let view = self.selectable_text_render_state(cx);
        let run = view.run(&text, rgb(color));
        view.render_styled_text_in_group(
            SelectableTextRole::PlainDocument,
            group,
            id,
            order,
            text,
            vec![run],
            Vec::new(),
            false,
        )
    }

    fn render_selectable_styled_text_in_group(
        &self,
        group: u64,
        id: u64,
        order: usize,
        text: SharedString,
        runs: Vec<TextRun>,
        cx: &App,
    ) -> AnyElement {
        self.selectable_text_render_state(cx)
            .render_styled_text_in_group(
                SelectableTextRole::PlainDocument,
                group,
                id,
                order,
                text,
                runs,
                Vec::new(),
                false,
            )
    }

    fn workspace_toolbar_action_button(
        &self,
        label: String,
        icon: Option<AnyElement>,
        options: ToolbarButtonOptions,
        listener: impl Fn(&MouseDownEvent, &mut Window, &mut App) + 'static,
    ) -> Div {
        oxideterm_gpui_ui::button::toolbar_action_button(
            &self.tokens,
            label,
            icon,
            options,
            listener,
        )
    }

    fn render_lucide_icon(icon: LucideIcon, size: f32, color: Rgba) -> AnyElement {
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
        color: Rgba,
    ) -> AnyElement {
        oxideterm_gpui_ui::motion::animated_chevron(
            &self.tokens,
            id,
            svg()
                .path(LucideIcon::ChevronRight.path())
                .size(px(size))
                .text_color(color),
            expanded,
        )
        .into_any_element()
    }

    fn render_loading_icon(
        &self,
        id: impl Into<gpui::ElementId>,
        size: f32,
        color: Rgba,
    ) -> AnyElement {
        oxideterm_gpui_ui::motion::animated_spinner(
            &self.tokens,
            id,
            svg()
                .path(LucideIcon::LoaderCircle.path())
                .size(px(size))
                .text_color(color),
        )
    }

    fn toggle_ai_thinking_with_motion(
        &mut self,
        key: String,
        default: bool,
        cx: &mut Context<Self>,
    ) {
        let expanded = self.state.update(cx, |ai, _| {
            ai.toggle_thinking_expansion(key.clone(), default);
            ai.chat_ui()
                .thinking_expansion_state
                .get(&key)
                .copied()
                .unwrap_or(default)
        });
        self.begin_disclosure_motion(format!("ai:{key}:thinking"), expanded, cx);
    }

    fn toggle_ai_tool_with_motion(&mut self, key: String, cx: &mut Context<Self>) {
        let expanded = self.state.update(cx, |ai, _| {
            ai.toggle_tool_call_expansion(key.clone());
            ai.chat_ui().tool_call_expansion_state.contains(&key)
        });
        self.begin_disclosure_motion(format!("ai:{key}:tool"), expanded, cx);
    }

    fn begin_disclosure_motion(&mut self, key: String, expanded: bool, cx: &mut Context<Self>) {
        if !self.tokens.motion.enabled {
            self.disclosure_motions.remove(&key);
            return;
        }
        let generation = self.disclosure_motions.begin(key.clone(), expanded);
        let delay = oxideterm_gpui_ui::motion::duration(
            &self.tokens,
            oxideterm_gpui_ui::motion::MotionDuration::Micro,
        );
        cx.spawn(async move |weak, cx| {
            cx.background_executor().timer(delay).await;
            let _ = weak.update(cx, |this, cx| {
                if this.disclosure_motions.finish(&key, generation) {
                    cx.notify();
                }
            });
        })
        .detach();
    }

    fn copy_ai_message(&self, a0: String, cx: &mut App) {
        let request = self.bindings.request.clone();
        self.state.update(cx, |ai, cx| {
            ai.copy_ai_message(
                a0,
                move |cx| request(AiTranscriptRequest::ClipboardReadFailed, None, cx),
                cx,
            );
        });
    }

    fn regenerate_ai_last_response(&self, cx: &mut App) {
        (self.bindings.request)(AiTranscriptRequest::Regenerate, None, cx);
    }

    fn request_delete_ai_message(&self, a0: String, cx: &mut App) {
        self.state
            .update(cx, |ai, cx| ai.request_delete_ai_message(a0, cx));
    }

    fn cancel_edit_ai_message(&self, cx: &mut App) {
        (self.bindings.request)(AiTranscriptRequest::CancelEdit, None, cx);
    }

    fn save_ai_message_edit(&self, cx: &mut App) {
        (self.bindings.request)(AiTranscriptRequest::SaveEdit, None, cx);
    }

    fn switch_ai_message_branch(&self, a0: String, a1: usize, cx: &mut App) {
        self.state
            .update(cx, |ai, cx| ai.switch_ai_message_branch(a0, a1, cx));
    }

    fn send_ai_follow_up_suggestion(&self, a0: String, cx: &mut App) {
        (self.bindings.request)(AiTranscriptRequest::FollowUp(a0), None, cx);
    }

    fn resolve_ai_tool_approval(
        &self,
        generation: u64,
        call: String,
        approved: bool,
        cx: &mut App,
    ) {
        self.state.update(cx, |state, cx| {
            state.resolve_tool_approval(generation, &call, approved);
            cx.notify();
        });
    }

    fn resolve_ai_acp_permission(&self, a0: u64, a1: String, a2: Option<String>, cx: &mut App) {
        (self.bindings.request)(AiTranscriptRequest::AcpPermission(a0, a1, a2), None, cx);
    }

    fn resolve_ai_tool_candidate_selection(
        &self,
        generation: u64,
        call: String,
        index: Option<usize>,
        cx: &mut App,
    ) {
        self.state.update(cx, |state, cx| {
            state.resolve_tool_candidate_selection(generation, &call, index);
            cx.notify();
        });
    }

    fn resolve_ai_cursor_request(&self, generation: u64, call: &str, accepted: bool, cx: &mut App) {
        let frame = self.state.read(cx).chat_stream_environment(cx);
        self.state.update(cx, |state, cx| {
            let Some((conversation, assistant, method)) = state
                .cursor_request(&(generation, call.into()))
                .map(|pending| {
                    let (conversation, assistant, request, _, _) = pending.view();
                    (
                        conversation.to_owned(),
                        assistant.to_owned(),
                        request.method(),
                    )
                })
            else {
                return;
            };
            if state.resolve_cursor_request(generation, call, accepted) {
                state.apply_ai_tool_status(
                    generation,
                    &conversation,
                    &assistant,
                    call,
                    method,
                    "{}",
                    if accepted { "completed" } else { "rejected" },
                    None,
                    Some("read".into()),
                    None,
                    false,
                    None,
                    None,
                    None,
                    |data| frame.diagnostic(data),
                    cx,
                );
                cx.notify();
            }
        });
    }

    fn insert_ai_code_block_command(&self, a0: String, cx: &mut App) {
        (self.bindings.request)(AiTranscriptRequest::InsertCommand(a0), None, cx);
    }

    fn retry_ai_chat_initialization(&self, cx: &mut App) {
        self.state.update(cx, |state, cx| {
            (self.bindings.initialize)(state, true, cx);
        });
    }
}

impl Render for AiAssistantSidebarView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if !self.refresh_frame(cx) {
            return div().into_any_element();
        }
        if self.frame().composer.enabled && !self.state.read(cx).chat_initialized() {
            self.state.update(cx, |state, cx| {
                (self.bindings.initialize)(state, false, cx);
            });
        }
        self.render_sidebar(cx)
    }
}

struct AiTextView {
    render: TextRenderer,
    font: SharedString,
}

impl AiTextView {
    fn run(&self, text: &str, color: Rgba) -> TextRun {
        TextRun {
            len: text.len(),
            font: font(self.font.clone()),
            color: color.into(),
            background_color: None,
            underline: None,
            strikethrough: None,
            letter_spacing: None,
        }
    }

    fn render_styled_text_in_group(
        &self,
        role: SelectableTextRole,
        group: u64,
        id: u64,
        order: usize,
        text: SharedString,
        runs: Vec<TextRun>,
        links: Vec<MarkdownTextLink>,
        join_previous: bool,
    ) -> AnyElement {
        if role == SelectableTextRole::NonSelectable {
            return div()
                .min_w(px(0.0))
                .child(StyledText::new(text).with_runs(runs))
                .into_any_element();
        }
        (self.render)(AiTextFragment {
            role,
            group,
            id,
            order,
            text,
            runs,
            links,
            join_previous,
        })
    }
}

fn selectable_text_id(scope: &str, key: impl Hash) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    scope.hash(&mut hasher);
    key.hash(&mut hasher);
    hasher.finish()
}

fn selectable_document_group_id() -> u64 {
    selectable_text_id("workspace-selectable-document", 0usize)
}

fn localized_ai_tool_value(i18n: &I18n, category: &str, value: &str) -> String {
    let key = format!("ai.tool_use.{category}.{value}");
    let localized = i18n.t(&key);
    if localized == key {
        value.to_owned()
    } else {
        localized
    }
}

const AI_MARKDOWN_WINDOW_OVERDRAW_PX: f32 = 720.0;
const AI_MARKDOWN_CONTENT_OFFSET_PX: f32 = 56.0;

#[derive(Clone, Debug)]
enum AiChatListItem {
    HistoryPage { older: bool },
    TrimNotice { count: usize },
    Message { index: usize, last_assistant: bool },
    BottomSpacer,
}

include!("transcript/list.rs");
include!("transcript/message.rs");
include!("transcript/agents.rs");
include!("transcript/sticky_prompt.rs");
include!("transcript/images.rs");
include!("transcript/markdown.rs");

include!("transcript/composer.rs");
include!("transcript/shell.rs");

pub enum AiSidebarLayerKind {
    Conversations(f32),
    ChatMenu,
    Reasoning,
    Safety,
    Context,
    Autocomplete(Vec<AiAutocompleteCandidate>),
    SafetyConfirm,
    SummaryConfirm,
    ClearAllConfirm,
    DeleteMessageConfirm,
}

#[derive(IntoElement)]
pub struct AiSidebarLayer {
    sidebar: Entity<AiAssistantSidebarView>,
    kind: AiSidebarLayerKind,
}

impl AiSidebarLayer {
    pub fn new(sidebar: &Entity<AiAssistantSidebarView>, kind: AiSidebarLayerKind) -> Self {
        Self {
            sidebar: sidebar.clone(),
            kind,
        }
    }
}

impl gpui::RenderOnce for AiSidebarLayer {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        self.sidebar.update(cx, |view, cx| {
            if !view.refresh_frame(cx) {
                return div().into_any_element();
            }
            match self.kind {
                AiSidebarLayerKind::Conversations(width) => {
                    view.render_ai_conversation_dropdown(width, cx)
                }
                AiSidebarLayerKind::ChatMenu => view.render_ai_chat_menu(cx),
                AiSidebarLayerKind::Reasoning => view
                    .render_ai_reasoning_menu(cx)
                    .unwrap_or_else(|| div().into_any_element()),
                AiSidebarLayerKind::Safety => view.render_ai_safety_menu(cx),
                AiSidebarLayerKind::Context => view.render_ai_context_popover(cx),
                AiSidebarLayerKind::Autocomplete(items) => {
                    view.render_ai_autocomplete_popup(&items, cx)
                }
                AiSidebarLayerKind::SafetyConfirm => view.render_ai_safety_confirm_dialog(cx),
                AiSidebarLayerKind::SummaryConfirm => view.render_ai_summarize_confirm_dialog(cx),
                AiSidebarLayerKind::ClearAllConfirm => view.render_ai_clear_all_confirm_dialog(cx),
                AiSidebarLayerKind::DeleteMessageConfirm => {
                    view.render_ai_delete_message_confirm_dialog(cx)
                }
            }
        })
    }
}

include!("transcript/overlays.rs");
include!("transcript/placement.rs");

impl AiTranscriptPolicy {
    pub fn compose(
        settings: &oxideterm_settings::AiSettings,
        ai: &AiWorkspaceEntity,
        config: impl FnOnce() -> Option<oxideterm_ai::AiChatStreamConfig>,
        context_counts: impl FnOnce() -> Option<(usize, usize)>,
        token_breakdown: impl FnOnce() -> crate::state::AiContextTokenBreakdown,
        [terminal_context, split_context, ide_context, sftp_context]: [bool; 4],
        tools: (bool, usize),
        auth_methods: Option<Vec<oxideterm_ai::AcpAuthMethod>>,
        rename: AiRenameFrame,
        confirm_focus: Option<oxideterm_gpui_ui::confirm::ConfirmDialogAction>,
    ) -> (AiComposerFrame, AiOverlayFrame, Vec<AgentModel>) {
        let policy = Self::from_settings(settings, ai, config);
        (
            AiComposerFrame {
                enabled: settings.enabled,
                model_selected: policy.model_selected,
                images_allowed: policy.images_allowed,
                backend: settings.active_backend,
                context_counts: settings.enabled.then(context_counts).flatten(),
                terminal_context,
                split_context,
                ide_context,
                sftp_context,
                tools,
                reasoning: policy.reasoning.as_ref().map(|(_, _, level)| *level),
                auth_methods,
            },
            AiOverlayFrame {
                rename,
                confirm_focus,
                reasoning: policy.reasoning,
                breakdown: ai.chat_ui().context_popover_open.then(token_breakdown),
            },
            policy.agent_models,
        )
    }
}
