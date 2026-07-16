use gpui::prelude::FluentBuilder;
use gpui::{
    AnyElement, App, Bounds, CursorStyle, Div, Element, ElementId, GlobalElementId,
    InspectorElementId, InteractiveElement, IntoElement, LayoutId, MouseButton, MouseDownEvent,
    ParentElement, Pixels, Stateful, StatefulInteractiveElement, Styled, Window, div, px, rgb,
    rgba, svg,
};
use oxideterm_theme::ThemeTokens;

use crate::{
    button::tauri_focus_visible_ring,
    menu::{menu_item_chrome, menu_surface_chrome},
};

const TAURI_SELECT_TRIGGER_BG_ALPHA: u32 = 0x80;
const TAURI_INLINE_SELECT_SELECTED_BG_ALPHA: u32 = 0x1f;
const TAURI_INLINE_SELECT_HIGHLIGHT_BG_ALPHA: u32 = 0x26;
const SELECT_CHEVRON_DOWN_PATH: &str = "lucide/chevron-down.svg";

#[derive(Clone, Copy, Debug, PartialEq)]
struct SelectTriggerChromeSpec {
    cursor: CursorStyle,
    opacity: f32,
    show_chevron: bool,
}

fn interactive_select_trigger_spec(disabled: bool) -> SelectTriggerChromeSpec {
    SelectTriggerChromeSpec {
        cursor: if disabled {
            CursorStyle::OperationNotAllowed
        } else {
            CursorStyle::PointingHand
        },
        opacity: if disabled { 0.5 } else { 1.0 },
        show_chevron: true,
    }
}

fn readonly_value_trigger_spec() -> SelectTriggerChromeSpec {
    // Read-only settings values borrow the select field chrome from Tauri, but
    // they must not advertise popup affordance or click ownership.
    SelectTriggerChromeSpec {
        cursor: CursorStyle::Arrow,
        opacity: 1.0,
        show_chevron: false,
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub enum SelectAnchorId {
    SettingsLanguage,
    SettingsUpdateChannel,
    SettingsAppearanceTheme,
    SettingsAppearanceDensity,
    SettingsAppearanceUiFontSizeSlider,
    SettingsAppearanceBorderRadiusSlider,
    VersionMigrationBorderRadiusSlider,
    SettingsAppearanceAnimation,
    SettingsAppearanceRenderProfile,
    SettingsAppearanceFrostedGlass,
    SettingsAppearanceBackgroundOpacitySlider,
    SettingsAppearanceBackgroundBlurSlider,
    SettingsAppearanceBackgroundFit,
    SettingsCustomThemeDuplicate,
    SettingsUpdateProxyMode,
    SettingsUpdateProxyProtocol,
    SettingsTerminalFontFamily,
    SettingsTerminalCjkFontFamily,
    SettingsTerminalFontSizeSlider,
    SettingsTerminalEncoding,
    SettingsTerminalCursorStyle,
    SettingsIdeAgentMode,
    SettingsLocalShell,
    SettingsLocalPrivilegeKind,
    SettingsConnectionIdleTimeout,
    SettingsReconnectMaxAttempts,
    SettingsReconnectBaseDelay,
    SettingsReconnectMaxDelay,
    SettingsNetworkProxyProtocol,
    SettingsNetworkProxyAuth,
    SettingsAiProviderTemplate,
    SettingsAiContextMaxChars,
    SettingsAiContextVisibleLines,
    SettingsAiGlobalReasoning,
    SettingsAiProviderReasoning(usize),
    SettingsAiModelReasoning(usize, usize),
    SettingsAiEmbeddingProvider,
    SettingsKnowledgeCollectionScope,
    SettingsKnowledgeDocumentFormat,
    SettingsAiMcpTransport,
    SettingsAiMcpAuthMode,
    SettingsSftpConcurrent,
    SettingsSftpDirectoryParallelism,
    SettingsSftpConflict,
    SettingsHighlightPreset,
    SettingsHighlightRenderMode(usize),
    AiPanelRoot,
    AiConversationList,
    AiChatMenu,
    AiModelSelector,
    AiInlineModelSelector,
    AiSafetyMenu,
    AiContextPopover,
    NewConnectionGroup,
    NewConnectionKeyAuthSource,
    NewConnectionManagedKey,
    NewConnectionJumpSavedConnection,
    NewConnectionJumpKeyAuthSource,
    NewConnectionJumpManagedKey,
    NewConnectionPrivilegeKind,
    NewConnectionUpstreamProxyPolicy,
    NewConnectionUpstreamProxyProtocol,
    NewConnectionUpstreamProxyAuth,
    NewConnectionSerialPort,
    NewConnectionSerialDataBits,
    NewConnectionSerialStopBits,
    NewConnectionSerialParity,
    NewConnectionSerialFlowControl,
    SettingsConnectionImportSource,
    SettingsConnectionImportDuplicateStrategy,
    CloudSyncBackend,
    CloudSyncAuthMode,
    CloudSyncConflictStrategy,
    IdeAgentStatus,
    TerminalBroadcastMenu,
    TerminalCommandBar,
    TerminalCwdMenu,
    TerminalGitBranchMenu,
    TerminalProjectMenu,
    TerminalCastSeekbar,
    SessionManagerViewMode,
    SessionManagerSort,
    SessionManagerBatchMove,
}

impl SelectAnchorId {
    pub fn is_settings_select_trigger(self) -> bool {
        // Settings SelectTrigger mirrors Radix: the popup can read the trigger
        // rect immediately on pointer-down. GPUI portal overlays need the same
        // rect prewarmed while closed so the first click does not wait for a
        // follow-up notify/prepaint cycle.
        matches!(
            self,
            Self::SettingsLanguage
                | Self::SettingsUpdateChannel
                | Self::SettingsUpdateProxyMode
                | Self::SettingsUpdateProxyProtocol
                | Self::SettingsAppearanceTheme
                | Self::SettingsAppearanceDensity
                | Self::SettingsAppearanceAnimation
                | Self::SettingsAppearanceRenderProfile
                | Self::SettingsAppearanceFrostedGlass
                | Self::SettingsAppearanceBackgroundFit
                | Self::SettingsCustomThemeDuplicate
                | Self::SettingsTerminalFontFamily
                | Self::SettingsTerminalCjkFontFamily
                | Self::SettingsTerminalEncoding
                | Self::SettingsTerminalCursorStyle
                | Self::SettingsIdeAgentMode
                | Self::SettingsLocalShell
                | Self::SettingsLocalPrivilegeKind
                | Self::SettingsConnectionIdleTimeout
                | Self::SettingsReconnectMaxAttempts
                | Self::SettingsReconnectBaseDelay
                | Self::SettingsReconnectMaxDelay
                | Self::SettingsNetworkProxyProtocol
                | Self::SettingsNetworkProxyAuth
                | Self::SettingsAiProviderTemplate
                | Self::SettingsAiContextMaxChars
                | Self::SettingsAiContextVisibleLines
                | Self::SettingsAiGlobalReasoning
                | Self::SettingsAiProviderReasoning(_)
                | Self::SettingsAiModelReasoning(_, _)
                | Self::SettingsAiEmbeddingProvider
                | Self::SettingsKnowledgeCollectionScope
                | Self::SettingsKnowledgeDocumentFormat
                | Self::SettingsAiMcpTransport
                | Self::SettingsAiMcpAuthMode
                | Self::SettingsSftpConcurrent
                | Self::SettingsSftpDirectoryParallelism
                | Self::SettingsSftpConflict
                | Self::SettingsHighlightPreset
                | Self::SettingsHighlightRenderMode(_)
                | Self::SettingsConnectionImportSource
                | Self::SettingsConnectionImportDuplicateStrategy
        )
    }

    pub fn is_new_connection_select_trigger(self) -> bool {
        // New-connection SelectTrigger is also rendered through a portal-style
        // overlay, so it needs the same closed-state anchor cache as settings.
        matches!(
            self,
            Self::NewConnectionGroup
                | Self::NewConnectionKeyAuthSource
                | Self::NewConnectionManagedKey
                | Self::NewConnectionJumpSavedConnection
                | Self::NewConnectionJumpKeyAuthSource
                | Self::NewConnectionJumpManagedKey
                | Self::NewConnectionPrivilegeKind
                | Self::NewConnectionUpstreamProxyPolicy
                | Self::NewConnectionUpstreamProxyProtocol
                | Self::NewConnectionUpstreamProxyAuth
                | Self::NewConnectionSerialPort
                | Self::NewConnectionSerialDataBits
                | Self::NewConnectionSerialStopBits
                | Self::NewConnectionSerialParity
                | Self::NewConnectionSerialFlowControl
        )
    }

    pub fn is_cloud_sync_select_trigger(self) -> bool {
        // Cloud Sync select popups are root-mounted like settings popups. Keep
        // their closed trigger rect warm so the first pointer click can open
        // immediately instead of waiting for a second prepaint cycle.
        matches!(
            self,
            Self::CloudSyncBackend | Self::CloudSyncAuthMode | Self::CloudSyncConflictStrategy
        )
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct OverlayAnchor {
    pub id: SelectAnchorId,
    pub bounds: Bounds<Pixels>,
}

type AnchorBoundsCallback = Box<dyn FnOnce(OverlayAnchor, &mut Window, &mut App)>;

pub struct SelectAnchorProbe {
    id: SelectAnchorId,
    child: Option<AnyElement>,
    on_bounds: Option<AnchorBoundsCallback>,
}

pub fn select_anchor_probe(
    id: SelectAnchorId,
    child: impl IntoElement,
    on_bounds: impl FnOnce(OverlayAnchor, &mut Window, &mut App) + 'static,
) -> SelectAnchorProbe {
    SelectAnchorProbe {
        id,
        child: Some(child.into_any_element()),
        on_bounds: Some(Box::new(on_bounds)),
    }
}

impl IntoElement for SelectAnchorProbe {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl Element for SelectAnchorProbe {
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
        let layout_id = self
            .child
            .as_mut()
            .expect("select anchor child should render once")
            .request_layout(window, cx);
        (layout_id, ())
    }

    fn prepaint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _request_layout: &mut Self::RequestLayoutState,
        window: &mut Window,
        cx: &mut App,
    ) -> Self::PrepaintState {
        if let Some(child) = self.child.as_mut() {
            child.prepaint(window, cx);
        }
        // Keep the anchor in the same draw pass as the trigger. Deferring this by a
        // frame lets scroll containers expose stale bounds to deferred popups.
        if let Some(on_bounds) = self.on_bounds.take() {
            let anchor = OverlayAnchor {
                id: self.id,
                bounds,
            };
            on_bounds(anchor, window, cx);
        }
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
        if let Some(child) = self.child.as_mut() {
            child.paint(window, cx);
        }
    }
}

pub fn select_trigger(
    tokens: &ThemeTokens,
    value: impl Into<String>,
    placeholder: bool,
    disabled: bool,
) -> Div {
    let spec = interactive_select_trigger_spec(disabled);
    select_trigger_chrome(
        tokens,
        value.into(),
        placeholder,
        spec.cursor,
        spec.opacity,
        spec.show_chevron,
    )
}

pub fn select_trigger_with_focus_visible(
    tokens: &ThemeTokens,
    value: impl Into<String>,
    placeholder: bool,
    disabled: bool,
    focused: bool,
) -> Div {
    // Tauri SelectTrigger owns both the base chrome and its focus-visible class.
    // Native callers still decide keyboard-vs-pointer focus ownership, but the
    // visual composition belongs in the shared select primitive.
    select_trigger_focus_visible(
        tokens,
        select_trigger(tokens, value, placeholder, disabled),
        focused,
    )
}

pub fn readonly_value_trigger(tokens: &ThemeTokens, value: impl Into<String>) -> Div {
    // Some settings rows use Select chrome only as a fixed value display. Keep
    // that styling on a separate primitive so read-only values do not inherit
    // popup ownership, disabled affordance, or clickable cursor semantics from
    // true SelectTrigger instances.
    let spec = readonly_value_trigger_spec();
    select_trigger_chrome(
        tokens,
        value.into(),
        false,
        spec.cursor,
        spec.opacity,
        spec.show_chevron,
    )
}

pub fn select_inline_trigger_chrome(
    tokens: &ThemeTokens,
    open: bool,
    focused: bool,
    focus_visible: bool,
) -> Div {
    // Tauri SelectTrigger is h-9, border-theme-border/50, bg-theme-bg/50,
    // px-3, text-sm, and only shows the ring for keyboard focus-visible.
    let trigger = div()
        .w_full()
        .h(px(tokens.metrics.ui_control_height))
        .min_w(px(0.0))
        .rounded(px(tokens.radii.md))
        .border_1()
        .border_color(if open || focused {
            rgb(tokens.ui.accent)
        } else {
            rgba((tokens.ui.border << 8) | TAURI_SELECT_TRIGGER_BG_ALPHA)
        })
        .bg(rgba((tokens.ui.bg << 8) | TAURI_SELECT_TRIGGER_BG_ALPHA))
        .px(px(tokens.metrics.ui_control_padding_x))
        .flex()
        .items_center()
        .justify_between()
        .overflow_hidden()
        .text_size(px(tokens.metrics.ui_text_sm))
        .text_color(rgb(tokens.ui.text))
        .cursor_pointer();

    select_trigger_focus_visible(tokens, trigger, focus_visible)
}

pub fn select_inline_menu(tokens: &ThemeTokens) -> Div {
    // Inline menus are rendered in-flow instead of through a portal, but still
    // use SelectContent's panel chrome and event-island behavior.
    select_event_boundary(
        div()
            .w_full()
            .rounded(px(tokens.radii.md))
            .border_1()
            .border_color(rgb(tokens.ui.border))
            .bg(rgb(tokens.ui.bg_panel))
            .overflow_hidden(),
    )
}

pub fn select_inline_option_row(tokens: &ThemeTokens, selected: bool, highlighted: bool) -> Div {
    // Mirrors Radix SelectItem focus:bg-theme-bg-hover and selected indicator
    // semantics while letting callers render selectable/nonselectable labels.
    div()
        .w_full()
        .h(px(tokens.metrics.ui_control_height))
        .px(px(tokens.metrics.ui_control_padding_x))
        .flex()
        .items_center()
        .justify_between()
        .text_size(px(tokens.metrics.ui_text_sm))
        .text_color(if selected {
            rgb(tokens.ui.accent)
        } else {
            rgb(tokens.ui.text)
        })
        .bg(if highlighted {
            rgba((tokens.ui.bg_hover << 8) | TAURI_INLINE_SELECT_HIGHLIGHT_BG_ALPHA)
        } else if selected {
            rgba((tokens.ui.accent << 8) | TAURI_INLINE_SELECT_SELECTED_BG_ALPHA)
        } else {
            rgba(0x00000000)
        })
        .cursor_pointer()
        .hover(|style| style.bg(rgb(tokens.ui.bg_hover)))
}

fn select_trigger_chrome(
    tokens: &ThemeTokens,
    value: String,
    placeholder: bool,
    cursor: CursorStyle,
    opacity: f32,
    show_chevron: bool,
) -> Div {
    div()
        .h(px(tokens.metrics.ui_control_height))
        .w_full()
        .min_w(px(0.0))
        .flex()
        .items_center()
        .justify_between()
        .overflow_hidden()
        .rounded(px(tokens.radii.md))
        .border_1()
        .border_color(rgba(
            (tokens.ui.border << 8) | TAURI_SELECT_TRIGGER_BG_ALPHA,
        ))
        .bg(rgba((tokens.ui.bg << 8) | TAURI_SELECT_TRIGGER_BG_ALPHA))
        .px(px(tokens.metrics.ui_control_padding_x))
        .text_size(px(tokens.metrics.ui_text_sm))
        .text_color(rgb(if placeholder {
            tokens.ui.text_muted
        } else {
            tokens.ui.text
        }))
        .opacity(opacity)
        // Native select-like controls share the same chrome. The caller owns
        // whether this is an interactive select cursor or a read-only display.
        .cursor(cursor)
        .child(div().flex_1().min_w(px(0.0)).truncate().child(value))
        .when(show_chevron, |trigger| {
            trigger.child(
                div()
                    .ml(px(tokens.spacing.two))
                    .size(px(tokens.metrics.ui_select_check_size))
                    .flex_none()
                    .flex()
                    .items_center()
                    .justify_center()
                    .text_color(rgb(tokens.ui.text_muted))
                    .opacity(0.5)
                    // Use the shared Lucide asset instead of a font glyph so
                    // select affordances render consistently across locales.
                    .child(
                        svg()
                            .path(SELECT_CHEVRON_DOWN_PATH)
                            .size(px(tokens.metrics.ui_select_check_size))
                            .text_color(rgb(tokens.ui.text_muted)),
                    ),
            )
        })
}

pub fn select_trigger_focus_visible(tokens: &ThemeTokens, trigger: Div, focused: bool) -> Div {
    if !focused {
        return trigger;
    }
    // Tauri SelectTrigger gets the same focus-visible ring as shadcn Button.
    // Native select owners pass keyboard focus explicitly so mouse-opened
    // dropdowns do not show the keyboard ring.
    trigger.shadow(tauri_focus_visible_ring(tokens))
}

pub fn select_popup(tokens: &ThemeTokens, width: f32) -> Stateful<Div> {
    select_popup_with_max_height(tokens, width, tokens.metrics.ui_select_max_height)
}

pub fn select_popup_with_max_height(
    tokens: &ThemeTokens,
    width: f32,
    max_height: f32,
) -> Stateful<Div> {
    // Radix SelectContent is portal-hosted and pointer/wheel input inside it
    // does not bubble to the trigger row, scroll container, or terminal behind.
    menu_surface_chrome(tokens)
        .id("select-popup-scroll")
        .min_w(px(width.max(tokens.metrics.ui_select_min_width)))
        .max_h(px(max_height))
        .overflow_y_scroll()
        .p(px(tokens.metrics.ui_menu_padding))
        .on_mouse_down(MouseButton::Left, |_event, _window, cx| {
            cx.stop_propagation();
        })
        .on_mouse_down(MouseButton::Right, |_event, _window, cx| {
            cx.stop_propagation();
        })
        .on_scroll_wheel(|_, _, cx| cx.stop_propagation())
}

pub fn select_event_boundary(menu: Div) -> Div {
    // Custom select-like popups share SelectContent's event-island semantics
    // even when a feature needs bespoke sizing or positioning.
    menu.on_mouse_down(MouseButton::Left, |_event, _window, cx| {
        cx.stop_propagation();
    })
    .on_mouse_down(MouseButton::Right, |_event, _window, cx| {
        cx.stop_propagation();
    })
    .on_scroll_wheel(|_, _, cx| cx.stop_propagation())
}

pub fn select_panel_popup_with_max_height(
    tokens: &ThemeTokens,
    width: f32,
    max_height: f32,
) -> Stateful<Div> {
    select_popup_with_max_height(tokens, width, max_height).bg(rgb(tokens.ui.bg_panel))
}

pub fn select_overlay_popup(tokens: &ThemeTokens, width: f32) -> Stateful<Div> {
    select_popup(tokens, width)
}

pub fn select_overlay_popup_with_max_height(
    tokens: &ThemeTokens,
    width: f32,
    max_height: f32,
) -> Stateful<Div> {
    select_popup_with_max_height(tokens, width, max_height)
}

pub fn select_panel_overlay_popup_with_max_height(
    tokens: &ThemeTokens,
    width: f32,
    max_height: f32,
) -> Stateful<Div> {
    select_panel_popup_with_max_height(tokens, width, max_height)
}

pub fn select_option(tokens: &ThemeTokens, label: impl Into<String>, selected: bool) -> Div {
    select_item(tokens, label, selected)
        .cursor_pointer()
        .hover(|item| item.bg(rgb(tokens.ui.bg_hover)))
}

pub fn select_option_highlighted(
    tokens: &ThemeTokens,
    label: impl Into<String>,
    selected: bool,
    highlighted: bool,
) -> Div {
    // Radix SelectItem keeps keyboard highlight separate from the selected
    // checkmark. Expose that state for portal/select popups that are not using
    // the inline select row primitive.
    select_option(tokens, label, selected).when(highlighted && !selected, |item| {
        item.bg(rgba(
            (tokens.ui.bg_hover << 8) | TAURI_INLINE_SELECT_HIGHLIGHT_BG_ALPHA,
        ))
    })
}

pub fn select_option_is_actionable(disabled: bool, loading: bool) -> bool {
    // Radix SelectItem and browser-backed option rows do not invoke selection
    // while disabled. Loading rows use the same guard because the label may
    // remain visible while async state settles.
    !(disabled || loading)
}

pub fn select_option_action(
    option: Div,
    disabled: bool,
    loading: bool,
    listener: impl Fn(&MouseDownEvent, &mut Window, &mut App) + 'static,
) -> Div {
    if select_option_is_actionable(disabled, loading) {
        option.on_mouse_down(MouseButton::Left, listener)
    } else {
        option.cursor(CursorStyle::OperationNotAllowed)
    }
}

pub fn select_content(tokens: &ThemeTokens) -> Div {
    menu_surface_chrome(tokens)
        .relative()
        .max_h(px(tokens.metrics.ui_select_max_height))
        .min_w(px(tokens.metrics.ui_select_min_width))
        .child(div().p(px(tokens.metrics.ui_menu_padding)))
}

pub fn select_item(tokens: &ThemeTokens, label: impl Into<String>, selected: bool) -> Div {
    menu_item_chrome(
        tokens,
        tokens.metrics.ui_menu_item_padding_x,
        tokens.metrics.ui_menu_inset_padding_left,
    )
    .w_full()
    .min_w(px(0.0))
    .when(selected, |item| {
        item.bg(rgba((tokens.ui.bg_hover << 8) | 0x80))
    })
    .child(
        div()
            .absolute()
            .right(px(tokens.metrics.ui_menu_item_padding_x))
            .size(px(tokens.metrics.ui_select_check_size))
            .flex()
            .items_center()
            .justify_center()
            .child(if selected { "✓" } else { "" }),
    )
    .child(label.into())
}

pub fn select_label(tokens: &ThemeTokens, label: impl Into<String>) -> Div {
    let label = label.into().to_uppercase();
    div()
        .px(px(tokens.metrics.ui_menu_item_padding_x))
        .py(px(tokens.metrics.ui_menu_item_padding_y))
        .text_size(px(tokens.metrics.ui_text_xs))
        .font_weight(gpui::FontWeight::BOLD)
        .text_color(rgb(tokens.ui.text_muted))
        .child(label)
}

pub fn select_separator(tokens: &ThemeTokens) -> Div {
    div()
        .mx(px(-tokens.metrics.ui_menu_padding))
        .my(px(tokens.metrics.ui_menu_padding))
        .h(px(1.0))
        .bg(rgb(tokens.ui.border))
}

#[cfg(test)]
mod tests {
    use gpui::CursorStyle;

    use super::{
        SelectAnchorId, interactive_select_trigger_spec, readonly_value_trigger_spec,
        select_option_is_actionable,
    };

    #[test]
    fn select_option_action_guard_blocks_disabled_or_loading_rows() {
        assert!(select_option_is_actionable(false, false));
        assert!(!select_option_is_actionable(true, false));
        assert!(!select_option_is_actionable(false, true));
        assert!(!select_option_is_actionable(true, true));
    }

    #[test]
    fn readonly_value_trigger_does_not_expose_select_affordance() {
        let readonly = readonly_value_trigger_spec();
        assert_eq!(readonly.cursor, CursorStyle::Arrow);
        assert_eq!(readonly.opacity, 1.0);
        assert!(!readonly.show_chevron);

        let interactive = interactive_select_trigger_spec(false);
        assert_eq!(interactive.cursor, CursorStyle::PointingHand);
        assert_eq!(interactive.opacity, 1.0);
        assert!(interactive.show_chevron);
    }

    #[test]
    fn settings_select_anchor_ids_are_distinct_from_slider_and_sidebar_anchors() {
        assert!(SelectAnchorId::SettingsLanguage.is_settings_select_trigger());
        assert!(SelectAnchorId::SettingsAiModelReasoning(1, 3).is_settings_select_trigger());
        assert!(SelectAnchorId::SettingsSftpConflict.is_settings_select_trigger());
        assert!(SelectAnchorId::SettingsConnectionImportSource.is_settings_select_trigger());
        assert!(
            SelectAnchorId::SettingsConnectionImportDuplicateStrategy.is_settings_select_trigger()
        );

        assert!(!SelectAnchorId::SettingsTerminalFontSizeSlider.is_settings_select_trigger());
        assert!(!SelectAnchorId::AiModelSelector.is_settings_select_trigger());
        assert!(!SelectAnchorId::AiInlineModelSelector.is_settings_select_trigger());
        assert!(!SelectAnchorId::NewConnectionGroup.is_settings_select_trigger());
    }

    #[test]
    fn new_connection_select_anchor_ids_are_tracked_as_trigger_anchors() {
        assert!(SelectAnchorId::NewConnectionGroup.is_new_connection_select_trigger());
        assert!(SelectAnchorId::NewConnectionKeyAuthSource.is_new_connection_select_trigger());
        assert!(SelectAnchorId::NewConnectionPrivilegeKind.is_new_connection_select_trigger());
        assert!(SelectAnchorId::NewConnectionJumpKeyAuthSource.is_new_connection_select_trigger());
        assert!(
            SelectAnchorId::NewConnectionUpstreamProxyPolicy.is_new_connection_select_trigger()
        );
        assert!(
            SelectAnchorId::NewConnectionUpstreamProxyProtocol.is_new_connection_select_trigger()
        );
        assert!(SelectAnchorId::NewConnectionUpstreamProxyAuth.is_new_connection_select_trigger());
        assert!(SelectAnchorId::NewConnectionSerialPort.is_new_connection_select_trigger());
        assert!(!SelectAnchorId::SettingsLanguage.is_new_connection_select_trigger());
        assert!(!SelectAnchorId::AiModelSelector.is_new_connection_select_trigger());
    }

    #[test]
    fn cloud_sync_select_anchor_ids_are_tracked_as_trigger_anchors() {
        assert!(SelectAnchorId::CloudSyncBackend.is_cloud_sync_select_trigger());
        assert!(SelectAnchorId::CloudSyncAuthMode.is_cloud_sync_select_trigger());
        assert!(SelectAnchorId::CloudSyncConflictStrategy.is_cloud_sync_select_trigger());

        assert!(!SelectAnchorId::SettingsLanguage.is_cloud_sync_select_trigger());
        assert!(!SelectAnchorId::AiModelSelector.is_cloud_sync_select_trigger());
    }
}
