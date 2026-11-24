use std::{ops::Range, rc::Rc};

use gpui::{CursorStyle, MouseButton, MouseDownEvent, MouseMoveEvent, Window};
use oxideterm_gpui_ui::{
    select::{
        BrowserFocusOrigin, OverlayAnchor, browser_focus_visible, select_anchor_probe,
        select_trigger_with_focus_visible,
    },
    text_input::{
        TextInputAnchor, TextInputAnchorId, TextInputContentAlign, TextInputView,
        TextInputViewport, text_input_anchor_probe, text_input_with_viewport_and_content_align,
    },
};

use super::*;
use crate::{SettingsInput, SettingsSelect, SettingsSelectAnchorExt, SettingsSlider};

pub trait SettingsImeSource {
    fn frame<'a>(
        &'a self,
        input: SettingsInput,
        value: &'a str,
        cx: &'a App,
    ) -> SettingsImeFrame<'a>;
}

pub type SettingsControlsSource = Rc<dyn for<'a> Fn(&'a App) -> Option<SettingsControls<'a>>>;

pub type SettingsLabelRenderer = Rc<dyn Fn(&str, &str, String, u32, &App) -> AnyElement>;

pub fn is_connection_key_input(input: SettingsInput) -> bool {
    matches!(
        input,
        SettingsInput::ManagedKeyFilePath
            | SettingsInput::ManagedKeyFileName
            | SettingsInput::ManagedKeyFilePassphrase
            | SettingsInput::ManagedKeyPasteName
            | SettingsInput::ManagedKeyPastePrivateKey
            | SettingsInput::ManagedKeyPastePassphrase
            | SettingsInput::ManagedKeyRenameName
    )
}

pub fn is_surface_input(input: SettingsInput) -> bool {
    matches!(
        input,
        SettingsInput::SettingsSearch
            | SettingsInput::SkillSearch
            | SettingsInput::AiToolUseMaxRounds
            | SettingsInput::AiModelContextWindow(_, _)
            | SettingsInput::KeybindingSearch
            | SettingsInput::KnownHostsSearch
            | SettingsInput::ConnectionImportTargetGroup
            | SettingsInput::NetworkProxyPassword
            | SettingsInput::NetworkProxyTestHost
            | SettingsInput::NetworkProxyTestPort
            | SettingsInput::LocalPrivilegeLabel
            | SettingsInput::LocalPrivilegeUsernameHint
            | SettingsInput::LocalPrivilegeSecret
            | SettingsInput::LocalPrivilegePromptPatterns
            | SettingsInput::LocalPrivilegeHostPatterns
            | SettingsInput::LocalPrivilegeSecondFactorPatterns
            | SettingsInput::LocalPrivilegeSecondFactorUserHint
            | SettingsInput::TotpName
            | SettingsInput::TotpSecret
            | SettingsInput::TotpPattern
            | SettingsInput::TotpPeriod
    )
}

/// Borrowed window services for building settings controls in the current frame.
#[derive(Clone)]
pub struct SettingsControls<'a> {
    pub components: SettingsComponents<'a>,
    pub open_select: Option<SettingsSelect>,
    pub select_origin: Option<BrowserFocusOrigin>,
    pub dispatch: SettingsWindowDispatch,
    pub ime: &'a dyn SettingsImeSource,
    pub label: SettingsLabelRenderer,
}

impl<'a> std::ops::Deref for SettingsControls<'a> {
    type Target = SettingsComponents<'a>;
    fn deref(&self) -> &Self::Target {
        &self.components
    }
}

impl SettingsControls<'_> {
    pub fn field(
        &self,
        input: SettingsInput,
        value: &str,
        placeholder: String,
        width: Option<f32>,
        align: TextInputContentAlign,
        secret: bool,
        cx: &App,
    ) -> AnyElement {
        self.text_field(
            self.ime.frame(input, value, cx),
            placeholder,
            align,
            secret,
            |input| {
                input
                    .when_some(width, |input, width| input.w(px(width)).max_w_full())
                    .when(width.is_none(), |input| input.w_full())
                    .min_w(px(0.0))
            },
            self.dispatch.clone(),
        )
    }

    pub fn settings_select_control(
        &self,
        select: SettingsSelect,
        value: String,
        disabled: bool,
        width: Option<f32>,
        _: &App,
    ) -> AnyElement {
        let resolved_width = width.map(|min_width| {
            oxideterm_gpui_ui::select::estimate_select_trigger_width(
                self.tokens,
                min_width,
                360.0,
                [value.as_str()],
            )
        });
        let trigger = self.select_trigger(
            value,
            false,
            disabled,
            self.open_select == Some(select),
            self.select_origin,
        );
        self.select_control(
            select,
            disabled,
            resolved_width,
            trigger,
            self.dispatch.clone(),
        )
    }

    pub fn setting_row(
        &self,
        label: &str,
        hint: &str,
        control: AnyElement,
        cx: &App,
    ) -> AnyElement {
        let text = self.tokens.ui;
        crate::settings_row(
            self.tokens,
            (self.label)(
                "settings-row-label",
                label,
                self.i18n.t(label),
                text.text,
                cx,
            ),
            (!hint.is_empty()).then(|| {
                (self.label)(
                    "settings-row-hint",
                    hint,
                    self.i18n.t(hint),
                    text.text_muted,
                    cx,
                )
            }),
            control,
        )
    }

    pub fn bool_row(
        &self,
        label: &str,
        hint: &str,
        checked: bool,
        setter: fn(&mut PersistedSettings, bool),
        cx: &App,
    ) -> AnyElement {
        self.setting_row(
            label,
            hint,
            self.toggle_control(label, checked, setter, self.dispatch.clone()),
            cx,
        )
    }

    pub fn settings_text_input_control_fill(
        &self,
        input: SettingsInput,
        value: impl AsRef<str>,
        placeholder: String,
        cx: &App,
    ) -> AnyElement {
        self.field(
            input,
            value.as_ref(),
            placeholder,
            None,
            TextInputContentAlign::Start,
            false,
            cx,
        )
    }

    pub fn settings_text_input_control(
        &self,
        input: SettingsInput,
        value: impl AsRef<str>,
        placeholder: String,
        width: f32,
        cx: &App,
    ) -> AnyElement {
        self.field(
            input,
            value.as_ref(),
            placeholder,
            Some(width),
            TextInputContentAlign::Start,
            false,
            cx,
        )
    }

    pub fn number_input(
        &self,
        input: SettingsInput,
        value: String,
        width: f32,
        cx: &App,
    ) -> AnyElement {
        self.field(
            input,
            &value,
            value.clone(),
            Some(width),
            TextInputContentAlign::Center,
            false,
            cx,
        )
    }

    pub fn number_row(
        &self,
        label: &str,
        hint: &str,
        input: SettingsInput,
        value: i64,
        cx: &App,
    ) -> AnyElement {
        self.decimal_row(label, hint, input, value.to_string(), cx)
    }

    pub fn decimal_row(
        &self,
        label: &str,
        hint: &str,
        input: SettingsInput,
        value: String,
        cx: &App,
    ) -> AnyElement {
        self.setting_row(
            label,
            hint,
            self.number_input(
                input,
                value,
                self.tokens.metrics.settings_number_input_width,
                cx,
            ),
            cx,
        )
    }

    pub fn select_setting_row(
        &self,
        label: &str,
        hint: &str,
        select: SettingsSelect,
        value: String,
        width: f32,
        cx: &App,
    ) -> AnyElement {
        let control = self.settings_select_control(select, value, false, Some(width), cx);
        self.setting_row(label, hint, control, cx)
    }
}

pub fn settings_mono_font_family(settings: &PersistedSettings) -> SharedString {
    use oxideterm_gpui_ui::typography::{css_font_family_head, gpui_font_family_name};
    let family = settings
        .terminal
        .font_family
        .terminal_family_name(&settings.terminal.custom_font_family);
    css_font_family_head(&family).unwrap_or_else(|| gpui_font_family_name(&family))
}

pub type SettingsEdit = Box<dyn FnOnce(&mut PersistedSettings)>;
pub type SettingsWindowDispatch = Rc<dyn Fn(SettingsWindowRequest, &mut Window, &mut App)>;

pub enum SettingsWindowRequest {
    ConfirmAiEnable,
    OpenAiTextEditor(crate::ai::text_editor::AiTextEditorDialog),
    ProbeAcpAgent(String),
    SetAiAgentConcurrency(usize),
    Edit(SettingsEdit),
    OpenSelect(SettingsSelect),
    SelectAnchor(OverlayAnchor),
    BeginInputSelection(SettingsInput, MouseDownEvent),
    DragInputSelection(MouseMoveEvent),
    DismissInput(SettingsInput),
    SliderAnchor(OverlayAnchor),
    BeginSlider(SettingsSlider, f32),
    MoveSlider(MouseMoveEvent),
    FinishSlider,
    PickSessionLogDirectory,
}

/// Borrow active text only while painting. The host and settings entity
/// retain draft ownership, including the zeroizing buffers of secret fields.
pub struct SettingsImeFrame<'a> {
    pub input: SettingsInput,
    pub value: &'a str,
    pub focused: bool,
    pub caret_visible: bool,
    pub selected_range: Option<Range<usize>>,
    pub active_offset: Option<usize>,
    pub marked_text: Option<&'a str>,
    pub anchor_id: TextInputAnchorId,
    pub viewport: TextInputViewport,
    pub update_anchor: Rc<dyn Fn(TextInputAnchor)>,
}

pub(crate) fn render_settings_select_control(
    select: SettingsSelect,
    disabled: bool,
    width: Option<f32>,
    trigger: Div,
    dispatch: SettingsWindowDispatch,
) -> AnyElement {
    let open = dispatch.clone();
    let trigger = trigger.when(!disabled, |trigger| {
        trigger
            .cursor_pointer()
            .on_mouse_down(MouseButton::Left, move |_, window, cx| {
                open(SettingsWindowRequest::OpenSelect(select), window, cx);
                cx.stop_propagation();
            })
    });
    div()
        .relative()
        .min_w(px(0.0))
        .when_some(width, |control, width| control.w(px(width)).max_w_full())
        .when(width.is_none(), |control| control.w_full())
        .child(select_anchor_probe(
            select.anchor_id(),
            trigger,
            move |anchor, window, cx| {
                dispatch(SettingsWindowRequest::SelectAnchor(anchor), window, cx)
            },
        ))
        .into_any_element()
}

pub(crate) fn render_settings_text_field(
    tokens: &ThemeTokens,
    frame: SettingsImeFrame<'_>,
    placeholder: String,
    align: TextInputContentAlign,
    secret: bool,
    style: impl FnOnce(Div) -> Div,
    dispatch: SettingsWindowDispatch,
) -> AnyElement {
    let target = frame.input;
    let input = text_input_with_viewport_and_content_align(
        tokens,
        TextInputView {
            value: frame.value,
            placeholder,
            focused: frame.focused,
            caret_visible: frame.caret_visible,
            secret,
            selected_all: false,
            selected_range: frame.selected_range,
            marked_text: frame.marked_text,
        },
        align,
        &frame.viewport,
        frame.active_offset,
    );
    let down = dispatch.clone();
    text_input_anchor_probe(
        frame.anchor_id,
        style(input)
            .cursor(CursorStyle::IBeam)
            .on_mouse_down(MouseButton::Left, move |event, window, cx| {
                down(
                    SettingsWindowRequest::BeginInputSelection(target, event.clone()),
                    window,
                    cx,
                );
                cx.stop_propagation();
            })
            .on_mouse_move(move |event, window, cx| {
                dispatch(
                    SettingsWindowRequest::DragInputSelection(event.clone()),
                    window,
                    cx,
                )
            }),
        move |anchor, _, _| (frame.update_anchor)(anchor),
    )
    .into_any_element()
}

impl SettingsComponents<'_> {
    pub fn navigation_search_field(
        &self,
        frame: SettingsImeFrame<'_>,
        dispatch: SettingsWindowDispatch,
    ) -> AnyElement {
        self.text_field(
            frame,
            self.i18n.t("settings_view.search.placeholder"),
            TextInputContentAlign::Start,
            false,
            |input| {
                input
                    .flex_1()
                    .min_w_0()
                    .h_full()
                    .px_0()
                    .border_0()
                    .rounded_none()
                    .bg(rgba(0x00000000))
                    .text_size(px(self.tokens.metrics.sidebar_title_font_size))
                    .line_height(px(20.0))
            },
            dispatch,
        )
    }

    pub fn select_trigger(
        &self,
        value: String,
        placeholder: bool,
        disabled: bool,
        focused: bool,
        origin: Option<BrowserFocusOrigin>,
    ) -> Div {
        select_trigger_with_focus_visible(
            self.tokens,
            value,
            placeholder,
            disabled,
            browser_focus_visible(focused, origin),
        )
    }

    pub fn select_control(
        &self,
        select: SettingsSelect,
        disabled: bool,
        width: Option<f32>,
        trigger: Div,
        dispatch: SettingsWindowDispatch,
    ) -> AnyElement {
        render_settings_select_control(select, disabled, width, trigger, dispatch)
    }

    pub fn text_field(
        &self,
        frame: SettingsImeFrame<'_>,
        placeholder: String,
        align: TextInputContentAlign,
        secret: bool,
        style: impl FnOnce(Div) -> Div,
        dispatch: SettingsWindowDispatch,
    ) -> AnyElement {
        render_settings_text_field(
            self.tokens,
            frame,
            placeholder,
            align,
            secret,
            style,
            dispatch,
        )
    }

    pub fn toggle_control(
        &self,
        label_key: &str,
        value: bool,
        set: fn(&mut PersistedSettings, bool),
        dispatch: SettingsWindowDispatch,
    ) -> AnyElement {
        oxideterm_gpui_ui::switch::switch(self.tokens, label_key.to_owned(), String::new(), value)
            .on_mouse_down(MouseButton::Left, move |_, window, cx| {
                dispatch(
                    SettingsWindowRequest::Edit(Box::new(move |settings| set(settings, !value))),
                    window,
                    cx,
                );
                cx.stop_propagation();
            })
            .into_any_element()
    }
}
