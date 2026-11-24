//! Hover affordance that lets a pointer user leave zen mode.
//!
//! Zen mode hides the title bar, both sidebars and the activity rails, so the
//! keyboard shortcut is otherwise the only way out. The affordance owns no GPUI
//! state: it tracks visibility plus a generation so a delayed hide cannot
//! resurrect itself after the pointer returns or zen mode ends.

use crate::{ShellEnvironment, ShellHost, ShellRequest};
use gpui::{AnyElement, Pixels, div, prelude::*, px, rgb, rgba};
use std::time::Duration;

const ZEN_EXIT_GAP: f32 = 4.0;

/// Invisible strip at the top edge that reveals the affordance.
pub const HOT_ZONE_HEIGHT: f32 = 6.0;
/// Grace period before the pill disappears once the pointer leaves it.
pub const HIDE_DELAY: Duration = Duration::from_millis(250);

#[derive(Default)]
pub struct ZenExitAffordance {
    visible: bool,
    pill_hovered: bool,
    hot_zone_hovered: bool,
    generation: u64,
}

impl ZenExitAffordance {
    pub fn visible(&self) -> bool {
        self.visible
    }

    /// Tracks whether the pointer is inside the pill itself. A hide scheduled by
    /// the hot strip's leave event must not fire while the pointer has already
    /// moved onto the pill, regardless of which hover event arrives first.
    pub fn set_pill_hovered(&mut self, hovered: bool) -> bool {
        if self.pill_hovered == hovered {
            return false;
        }
        self.pill_hovered = hovered;
        true
    }

    /// Tracks the reveal strip symmetrically. Moving from the pill back onto the
    /// strip can deliver `pill(false)` after `strip(true)`; without this flag the
    /// freshly scheduled hide would fire while the pointer still owns the strip.
    pub fn set_hot_zone_hovered(&mut self, hovered: bool) -> bool {
        if self.hot_zone_hovered == hovered {
            return false;
        }
        self.hot_zone_hovered = hovered;
        true
    }

    /// Reveals the affordance. Returns `true` only for the transition from
    /// hidden to visible so callers run the one-shot side effects once.
    ///
    /// Every call inside zen mode also renews the generation, so a repeat reveal
    /// while already visible cancels any hide scheduled by `schedule_hide`.
    pub fn reveal(&mut self, zen_mode: bool) -> bool {
        if !zen_mode {
            // Both call sites are gated on zen mode today; keep the state machine
            // self-healing so a future caller cannot pin a stale affordance.
            self.reset();
            return false;
        }
        self.generation = self.generation.wrapping_add(1);
        if self.visible {
            return false;
        }
        self.visible = true;
        true
    }

    /// Starts a delayed hide and returns the generation that hide must carry.
    pub fn schedule_hide(&mut self) -> Option<u64> {
        if !self.visible {
            return None;
        }
        self.generation = self.generation.wrapping_add(1);
        Some(self.generation)
    }

    /// Applies a delayed hide. Stale generations are ignored, which is what
    /// cancels the timer when the pointer comes back.
    pub fn apply_hide(&mut self, generation: u64) -> bool {
        if !self.visible
            || self.pill_hovered
            || self.hot_zone_hovered
            || generation != self.generation
        {
            return false;
        }
        self.visible = false;
        true
    }

    /// Drops the affordance and invalidates every pending hide.
    pub fn reset(&mut self) {
        self.visible = false;
        self.pill_hovered = false;
        self.hot_zone_hovered = false;
        self.generation = self.generation.wrapping_add(1);
    }
}

impl ZenExitAffordance {
    /// The top strip provides a pointer exit when zen mode hides the title bar.
    pub fn hot_zone<H: ShellHost>(&self, top: Pixels, cx: &Context<H>) -> AnyElement {
        // Once revealed, keep hovering across the visual gap without capturing clicks there.
        let height = HOT_ZONE_HEIGHT + if self.visible() { ZEN_EXIT_GAP } else { 0.0 };
        div()
            .id("zen-exit-hot-zone")
            .debug_selector(|| "zen-exit-hot-zone".into())
            .absolute()
            .top(top)
            .left_0()
            .right_0()
            .h(px(height))
            .on_hover(cx.listener(|this, hovered: &bool, window, cx| {
                // Leaving the strip without ever reaching the pill must still
                // retire it; a hover on either region keeps `apply_hide` from
                // firing.
                this.request(ShellRequest::HoverZenStrip(*hovered), window, cx);
            }))
            .into_any_element()
    }

    pub fn pill<H: ShellHost>(
        &self,
        top: Pixels,
        env: &ShellEnvironment,
        shortcut: String,
        cx: &Context<H>,
    ) -> AnyElement {
        let tokens = &env.tokens;
        let label = env.i18n.t("zen_mode.exit");
        let pill = div()
            .id("zen-exit-affordance-pill")
            .debug_selector(|| "zen-exit-affordance-pill".into())
            .flex()
            .items_center()
            .gap(px(8.0))
            .rounded(px(tokens.radii.md))
            .border_1()
            .border_color(rgb(tokens.ui.border))
            .bg(rgba((tokens.ui.bg_elevated << 8) | 0xe6))
            .px(px(14.0))
            .py(px(6.0))
            .text_size(px(13.0))
            .line_height(px(18.0))
            .text_color(rgb(tokens.ui.text))
            .shadow_lg()
            .cursor_pointer()
            .block_mouse_except_scroll()
            .on_click(
                cx.listener(|this, _, window, cx| this.request(ShellRequest::ExitZen, window, cx)),
            )
            .on_hover(cx.listener(|this, hovered: &bool, window, cx| {
                this.request(ShellRequest::HoverZenPill(*hovered), window, cx);
            }))
            .child(label)
            .when(!shortcut.is_empty(), |pill| {
                pill.child(
                    div()
                        .rounded(px(tokens.radii.sm))
                        .bg(rgba((tokens.ui.bg_hover << 8) | 0xff))
                        .px(px(6.0))
                        .py(px(1.0))
                        .text_size(px(11.0))
                        .text_color(rgb(tokens.ui.text_muted))
                        .child(shortcut),
                )
            });
        div()
            .absolute()
            .top(top + px(HOT_ZONE_HEIGHT + ZEN_EXIT_GAP))
            .left_0()
            .right_0()
            .flex()
            .justify_center()
            .child(oxideterm_gpui_ui::motion::fade_in(
                tokens,
                "zen-exit-affordance",
                pill,
                oxideterm_gpui_ui::motion::MotionDuration::Control,
            ))
            .into_any_element()
    }
}
