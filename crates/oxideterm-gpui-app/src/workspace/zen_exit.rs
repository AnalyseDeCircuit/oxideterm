//! Hover affordance that lets a pointer user leave zen mode.
//!
//! Zen mode hides the title bar, both sidebars and the activity rails, so the
//! keyboard shortcut is otherwise the only way out. The affordance owns no GPUI
//! state: it tracks visibility plus a generation so a delayed hide cannot
//! resurrect itself after the pointer returns or zen mode ends.

use std::time::Duration;

/// Invisible strip at the top edge that reveals the affordance.
pub(in crate::workspace) const HOT_ZONE_HEIGHT: f32 = 6.0;
/// Grace period before the pill disappears once the pointer leaves it.
pub(in crate::workspace) const HIDE_DELAY: Duration = Duration::from_millis(250);

#[derive(Default)]
pub(in crate::workspace) struct ZenExitAffordance {
    visible: bool,
    pill_hovered: bool,
    hot_zone_hovered: bool,
    generation: u64,
}

impl ZenExitAffordance {
    pub(in crate::workspace) fn visible(&self) -> bool {
        self.visible
    }

    /// Tracks whether the pointer is inside the pill itself. A hide scheduled by
    /// the hot strip's leave event must not fire while the pointer has already
    /// moved onto the pill, regardless of which hover event arrives first.
    pub(in crate::workspace) fn set_pill_hovered(&mut self, hovered: bool) -> bool {
        if self.pill_hovered == hovered {
            return false;
        }
        self.pill_hovered = hovered;
        true
    }

    /// Tracks the reveal strip symmetrically. Moving from the pill back onto the
    /// strip can deliver `pill(false)` after `strip(true)`; without this flag the
    /// freshly scheduled hide would fire while the pointer still owns the strip.
    pub(in crate::workspace) fn set_hot_zone_hovered(&mut self, hovered: bool) -> bool {
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
    pub(in crate::workspace) fn reveal(&mut self, zen_mode: bool) -> bool {
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
    pub(in crate::workspace) fn schedule_hide(&mut self) -> Option<u64> {
        if !self.visible {
            return None;
        }
        self.generation = self.generation.wrapping_add(1);
        Some(self.generation)
    }

    /// Applies a delayed hide. Stale generations are ignored, which is what
    /// cancels the timer when the pointer comes back.
    pub(in crate::workspace) fn apply_hide(&mut self, generation: u64) -> bool {
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
    pub(in crate::workspace) fn reset(&mut self) {
        self.visible = false;
        self.pill_hovered = false;
        self.hot_zone_hovered = false;
        self.generation = self.generation.wrapping_add(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reveal_only_happens_inside_zen_mode() {
        let mut affordance = ZenExitAffordance::default();
        assert!(!affordance.reveal(false));
        assert!(!affordance.visible());
        assert!(affordance.reveal(true));
        assert!(affordance.visible());
        assert!(
            !affordance.reveal(true),
            "repeat reveals are not transitions"
        );
    }

    #[test]
    fn pending_hide_is_cancelled_when_the_pointer_returns() {
        let mut affordance = ZenExitAffordance::default();
        assert!(affordance.reveal(true));
        let pending = affordance.schedule_hide().expect("visible affordance");
        // Returning to the pill while already visible still bumps the generation.
        assert!(!affordance.reveal(true));
        assert!(
            !affordance.apply_hide(pending),
            "stale hide must be ignored"
        );
        assert!(affordance.visible());
    }

    #[test]
    fn hide_applies_for_the_current_generation() {
        let mut affordance = ZenExitAffordance::default();
        assert!(affordance.reveal(true));
        let pending = affordance.schedule_hide().expect("visible affordance");
        assert!(affordance.apply_hide(pending));
        assert!(!affordance.visible());
    }

    #[test]
    fn reset_hides_and_invalidates_pending_hides() {
        let mut affordance = ZenExitAffordance::default();
        assert!(affordance.reveal(true));
        let pending = affordance.schedule_hide().expect("visible affordance");
        affordance.reset();
        assert!(!affordance.visible());
        assert!(!affordance.apply_hide(pending));
    }

    #[test]
    fn hide_waits_while_the_pointer_is_on_the_pill() {
        let mut affordance = ZenExitAffordance::default();
        assert!(affordance.reveal(true));
        // The hot strip can deliver its leave after the pill received enter.
        assert!(affordance.set_pill_hovered(true));
        let pending = affordance.schedule_hide().expect("visible affordance");
        assert!(!affordance.apply_hide(pending));
        assert!(affordance.visible());
        assert!(affordance.set_pill_hovered(false));
        let pending = affordance.schedule_hide().expect("still visible");
        assert!(affordance.apply_hide(pending));
        assert!(!affordance.visible());
    }

    #[test]
    fn revealing_outside_zen_mode_drops_a_stale_affordance() {
        let mut affordance = ZenExitAffordance::default();
        assert!(affordance.reveal(true));
        assert!(!affordance.reveal(false));
        assert!(!affordance.visible());
    }

    #[test]
    fn hide_waits_while_the_pointer_is_on_the_hot_strip() {
        let mut affordance = ZenExitAffordance::default();
        assert!(affordance.reveal(true));
        // Pill leave may be delivered after the strip's enter event.
        assert!(affordance.set_hot_zone_hovered(true));
        let pending = affordance.schedule_hide().expect("visible affordance");
        assert!(!affordance.apply_hide(pending));
        assert!(affordance.visible());
        assert!(affordance.set_hot_zone_hovered(false));
        let pending = affordance.schedule_hide().expect("still visible");
        assert!(affordance.apply_hide(pending));
        assert!(!affordance.visible());
    }
}
