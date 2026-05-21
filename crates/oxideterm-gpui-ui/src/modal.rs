use gpui::{
    AnyElement, Div, InteractiveElement, IntoElement, MouseButton, ParentElement, Rgba, Styled,
    div, px, rgb, rgba,
};
use oxideterm_theme::ThemeTokens;
use std::sync::atomic::{AtomicBool, Ordering};

const TW_BLACK: u32 = 0x000000;
const DIALOG_BACKDROP_ALPHA: u32 = 0x99; // Tauri DialogOverlay bg-black/60.
const COMMAND_PALETTE_BACKDROP_ALPHA: u32 = 0x66; // Tauri CommandPalette overlayClassName bg-black/40.
const QUICKLOOK_BACKDROP_ALPHA: u32 = 0xcc; // Tauri QuickLook bg-black/80.
const TRANSPARENT_BACKDROP_ALPHA: u32 = 0x00; // Radix popover outside-hit-test layer.
const TAILWIND_BACKDROP_BLUR_SM_PX: f32 = 4.0; // Tailwind backdrop-blur-sm.
static TAURI_BACKDROP_BLUR_ALLOWED: AtomicBool = AtomicBool::new(true);

// Tauri/Radix portals use z-50 for dialog/select/popover surfaces; GPUI uses
// deferred priority instead of CSS stacking, so keep the source z-index value
// named and centralized for every migrated floating layer.
pub const TAURI_POPOVER_LAYER_PRIORITY: usize = 50;
pub const TAURI_SELECT_LAYER_PRIORITY: usize = 50;
// Tauri TooltipContent uses z-[9999], intentionally above normal portals.
pub const TAURI_TOOLTIP_LAYER_PRIORITY: usize = 9_999;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TauriBackdropRole {
    Dialog,
    CommandPalette,
    QuickLook,
    Popover,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OverlayDismissPolicy {
    ProtectedDialog,
    DismissibleDialog,
    ProtectedCommandPalette,
    DismissibleCommandPalette,
    DismissibleQuickLook,
    DismissiblePopover,
}

impl OverlayDismissPolicy {
    pub fn backdrop_role(self) -> TauriBackdropRole {
        match self {
            Self::ProtectedDialog | Self::DismissibleDialog => TauriBackdropRole::Dialog,
            Self::ProtectedCommandPalette | Self::DismissibleCommandPalette => {
                TauriBackdropRole::CommandPalette
            }
            Self::DismissibleQuickLook => TauriBackdropRole::QuickLook,
            Self::DismissiblePopover => TauriBackdropRole::Popover,
        }
    }

    pub fn dismisses_on_outside_pointer(self) -> bool {
        // Protected auth/editor states still need the modal event island, but
        // outside pointer-down must not silently cancel them unless the Tauri
        // source proves that behavior for that exact dialog.
        !matches!(self, Self::ProtectedDialog | Self::ProtectedCommandPalette)
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TauriBackdropEffect {
    pub color: Rgba,
    pub blur_px: Option<f32>,
}

pub fn set_tauri_backdrop_blur_allowed(allowed: bool) {
    // Native render-profile changes are global app appearance state, the same
    // way Tauri writes data-frosted/data-animation attributes on the root.
    TAURI_BACKDROP_BLUR_ALLOWED.store(allowed, Ordering::Relaxed);
}

pub fn backdrop_source_effect(role: TauriBackdropRole) -> TauriBackdropEffect {
    let blur_px = match role {
        // Tauri DialogOverlay always includes linuxBackdropBlurClass("backdrop-blur-sm");
        // CommandPalette only overrides the overlay color, and QuickLook has its own
        // fixed overlay that also keeps the same blur class.
        TauriBackdropRole::Dialog
        | TauriBackdropRole::CommandPalette
        | TauriBackdropRole::QuickLook => Some(TAILWIND_BACKDROP_BLUR_SM_PX),
        TauriBackdropRole::Popover => None,
    };
    TauriBackdropEffect {
        color: backdrop_color(role),
        blur_px,
    }
}

pub fn backdrop_effect_with_blur_allowed(
    role: TauriBackdropRole,
    blur_allowed: bool,
) -> TauriBackdropEffect {
    let mut effect = backdrop_source_effect(role);
    if !blur_allowed {
        // Tauri disables these backdrop-blur classes on unsafe Linux webview
        // profiles. Native uses render-policy's blur allowance as the same
        // compatibility gate for GPUI top-layer effects.
        effect.blur_px = None;
    }
    effect
}

pub fn backdrop_effect(role: TauriBackdropRole) -> TauriBackdropEffect {
    backdrop_effect_with_blur_allowed(role, TAURI_BACKDROP_BLUR_ALLOWED.load(Ordering::Relaxed))
}

pub fn backdrop_color(role: TauriBackdropRole) -> Rgba {
    let alpha = match role {
        TauriBackdropRole::Dialog => DIALOG_BACKDROP_ALPHA,
        TauriBackdropRole::CommandPalette => COMMAND_PALETTE_BACKDROP_ALPHA,
        TauriBackdropRole::QuickLook => QUICKLOOK_BACKDROP_ALPHA,
        TauriBackdropRole::Popover => TRANSPARENT_BACKDROP_ALPHA,
    };
    rgba((TW_BLACK << 8) | alpha)
}

pub fn dialog_backdrop_color() -> Rgba {
    backdrop_color(TauriBackdropRole::Dialog)
}

pub fn quicklook_backdrop_color() -> Rgba {
    backdrop_color(TauriBackdropRole::QuickLook)
}

pub fn modal_overlay(tokens: &ThemeTokens, dialog: impl IntoElement) -> AnyElement {
    dialog_overlay(tokens, dialog)
}

pub fn modal_backdrop(backdrop: Rgba) -> Div {
    div()
        .absolute()
        .top_0()
        .left_0()
        .right_0()
        .bottom_0()
        .flex()
        .items_center()
        .justify_center()
        .bg(backdrop)
        .occlude()
        .on_mouse_down(MouseButton::Right, |_, _, cx| cx.stop_propagation())
        .on_scroll_wheel(|_, _, cx| cx.stop_propagation())
}

pub fn dialog_backdrop() -> Div {
    // Radix DialogOverlay is modal: pointer and wheel events cannot fall through
    // to the background surface while the dialog is open.
    modal_backdrop_for_policy(OverlayDismissPolicy::ProtectedDialog)
        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
}

fn modal_backdrop_for_policy(policy: OverlayDismissPolicy) -> Div {
    let effect = backdrop_effect(policy.backdrop_role());
    // Tauri paints these top-layer overlays with CSS backdrop-filter, which
    // samples the already-rendered app behind the element. GPUI 0.2.2 only has
    // window-background blur and shadow blur, so this currently paints the
    // source overlay color while keeping the requested blur radius classified
    // in `TauriBackdropEffect`. A real parity fix must wire `effect.blur_px`
    // through an order-aware renderer primitive that can sample the scene behind
    // this element; NSVisualEffectView/window blur and screen capture are not
    // equivalent because they blur outside-window content instead of the GPUI
    // scene behind the modal.
    modal_backdrop(effect.color)
}

fn dismissible_modal_backdrop(policy: OverlayDismissPolicy) -> Div {
    // Tauri shadcn/Radix Dialog keeps the overlay modal, but pointer-down on
    // the overlay itself drives onOpenChange(false). Callers attach their close
    // callback here and stop propagation on the dialog content.
    debug_assert!(policy.dismisses_on_outside_pointer());
    modal_backdrop_for_policy(policy)
}

pub fn dismissible_dialog_backdrop() -> Div {
    dismissible_modal_backdrop(OverlayDismissPolicy::DismissibleDialog)
}

pub fn command_palette_backdrop() -> Div {
    // Tauri CommandPalette overrides DialogOverlay with overlayClassName
    // "bg-black/40"; keep that as a named top-layer role instead of letting
    // feature code hand-pick a translucent black.
    modal_backdrop_for_policy(OverlayDismissPolicy::ProtectedCommandPalette)
}

pub fn dismissible_command_palette_backdrop() -> Div {
    // Same outside-dismiss contract as Radix Dialog, but with CommandPalette's
    // lighter overlayClassName rather than the default DialogOverlay color.
    dismissible_modal_backdrop(OverlayDismissPolicy::DismissibleCommandPalette)
}

pub fn quicklook_backdrop() -> Div {
    // Tauri QuickLook uses bg-black/80 with the same backdrop-blur-sm class as
    // DialogOverlay; left-clicking the backdrop itself closes the preview.
    dismissible_modal_backdrop(OverlayDismissPolicy::DismissibleQuickLook)
}

pub fn popover_backdrop() -> Div {
    // Radix popovers/context menus dismiss on outside pointer activity while
    // their portal content remains interactive.
    let role = OverlayDismissPolicy::DismissiblePopover.backdrop_role();
    div()
        .absolute()
        .top_0()
        .left_0()
        .right_0()
        .bottom_0()
        .bg(backdrop_color(role))
        .occlude()
        .on_scroll_wheel(|_, _, cx| cx.stop_propagation())
}

pub fn overlay_content_boundary<E>(content: E) -> E
where
    E: InteractiveElement,
{
    // Browser/Radix overlay content is an event island: pointer and wheel input
    // inside the panel must not bubble to the outside-dismiss layer or scroll
    // the terminal/page behind it.
    content
        .on_mouse_down(MouseButton::Left, |_event, _window, cx| {
            cx.stop_propagation();
        })
        .on_mouse_down(MouseButton::Right, |_event, _window, cx| {
            cx.stop_propagation();
        })
        .on_scroll_wheel(|_, _, cx| cx.stop_propagation())
}

pub fn dialog_overlay(_tokens: &ThemeTokens, dialog: impl IntoElement) -> AnyElement {
    dialog_backdrop().child(dialog).into_any_element()
}

pub fn modal_container(tokens: &ThemeTokens) -> Div {
    dialog_content(tokens)
}

pub fn dialog_content(tokens: &ThemeTokens) -> Div {
    let theme = tokens.ui;
    div()
        .w(px(tokens.metrics.modal_width))
        .rounded(px(tokens.radii.md))
        .overflow_hidden()
        .border_1()
        .border_color(rgb(theme.border))
        .bg(rgb(theme.bg_elevated))
}

pub fn modal_header(tokens: &ThemeTokens, title: String, subtitle: String) -> AnyElement {
    dialog_header(tokens)
        .child(dialog_title(tokens, title))
        .child(dialog_description(tokens, subtitle))
        .into_any_element()
}

pub fn dialog_header(tokens: &ThemeTokens) -> Div {
    let theme = tokens.ui;
    div()
        .flex()
        .flex_col()
        .flex_none()
        .justify_center()
        .px(px(tokens.metrics.modal_header_padding_x))
        .py(px(tokens.metrics.modal_header_padding_y))
        .bg(rgb(theme.bg_panel))
        .border_b_1()
        .border_color(rgb(theme.border))
}

pub fn dialog_title(tokens: &ThemeTokens, title: String) -> Div {
    div()
        .text_size(px(tokens.metrics.ui_text_sm))
        .font_weight(gpui::FontWeight::SEMIBOLD)
        .line_height(px(tokens.metrics.ui_text_sm))
        .text_color(rgb(tokens.ui.text_heading))
        .child(title)
}

pub fn dialog_description(tokens: &ThemeTokens, description: String) -> Div {
    div()
        .mt(px(tokens.spacing.one + tokens.spacing.one / 2.0))
        .text_size(px(tokens.metrics.ui_text_sm))
        .text_color(rgb(tokens.ui.text_muted))
        .child(description)
}

pub fn modal_body(tokens: &ThemeTokens) -> Div {
    div()
        .p(px(tokens.metrics.modal_body_padding))
        .flex()
        .flex_col()
        .gap(px(tokens.metrics.modal_body_gap))
}

pub fn modal_footer(tokens: &ThemeTokens) -> Div {
    dialog_footer(tokens)
}

pub fn dialog_footer(tokens: &ThemeTokens) -> Div {
    let theme = tokens.ui;
    div()
        .h(px(tokens.metrics.modal_footer_height))
        .px(px(tokens.metrics.modal_footer_padding_x))
        .flex()
        .flex_row()
        .items_center()
        .justify_end()
        .gap_2()
        .border_t_1()
        .border_color(rgb(theme.border))
        .bg(rgb(theme.bg_panel))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rgba_hex(color: Rgba) -> u32 {
        color.into()
    }

    #[test]
    fn backdrop_colors_match_tauri_overlay_classes() {
        // These values mirror Tauri bg-black/{60,40,80} and the transparent
        // outside-hit-test layer used for popovers.
        assert_eq!(
            rgba_hex(backdrop_color(TauriBackdropRole::Dialog)),
            0x00000099
        );
        assert_eq!(
            rgba_hex(backdrop_color(TauriBackdropRole::CommandPalette)),
            0x00000066
        );
        assert_eq!(
            rgba_hex(backdrop_color(TauriBackdropRole::QuickLook)),
            0x000000cc
        );
        assert_eq!(
            rgba_hex(backdrop_color(TauriBackdropRole::Popover)),
            0x00000000
        );
    }

    #[test]
    fn backdrop_blur_sources_match_tauri_classes() {
        // Dialog, CommandPalette, and QuickLook all keep
        // linuxBackdropBlurClass("backdrop-blur-sm") in the Tauri source.
        assert_eq!(
            backdrop_source_effect(TauriBackdropRole::Dialog).blur_px,
            Some(TAILWIND_BACKDROP_BLUR_SM_PX)
        );
        assert_eq!(
            backdrop_source_effect(TauriBackdropRole::CommandPalette).blur_px,
            Some(TAILWIND_BACKDROP_BLUR_SM_PX)
        );
        assert_eq!(
            backdrop_source_effect(TauriBackdropRole::QuickLook).blur_px,
            Some(TAILWIND_BACKDROP_BLUR_SM_PX)
        );
        assert_eq!(
            backdrop_source_effect(TauriBackdropRole::Popover).blur_px,
            None
        );
    }

    #[test]
    fn backdrop_blur_can_be_disabled_by_render_policy() {
        // This mirrors Tauri's Linux safe-profile helper and native
        // render-profile compatibility mode: colors stay identical while the
        // blur request is stripped before painting.
        assert_eq!(
            backdrop_effect_with_blur_allowed(TauriBackdropRole::Dialog, false).color,
            backdrop_source_effect(TauriBackdropRole::Dialog).color
        );
        assert_eq!(
            backdrop_effect_with_blur_allowed(TauriBackdropRole::Dialog, false).blur_px,
            None
        );
        assert_eq!(
            backdrop_effect_with_blur_allowed(TauriBackdropRole::CommandPalette, true).blur_px,
            Some(TAILWIND_BACKDROP_BLUR_SM_PX)
        );
    }

    #[test]
    fn dismiss_policy_keeps_backdrop_role_and_outside_close_separate() {
        assert_eq!(
            OverlayDismissPolicy::ProtectedDialog.backdrop_role(),
            TauriBackdropRole::Dialog
        );
        assert!(!OverlayDismissPolicy::ProtectedDialog.dismisses_on_outside_pointer());

        assert_eq!(
            OverlayDismissPolicy::DismissibleCommandPalette.backdrop_role(),
            TauriBackdropRole::CommandPalette
        );
        assert!(OverlayDismissPolicy::DismissibleCommandPalette.dismisses_on_outside_pointer());

        assert_eq!(
            OverlayDismissPolicy::DismissiblePopover.backdrop_role(),
            TauriBackdropRole::Popover
        );
        assert!(OverlayDismissPolicy::DismissiblePopover.dismisses_on_outside_pointer());
    }
}
