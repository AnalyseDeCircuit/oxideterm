pub(in crate::workspace) use oxideterm_gpui_shell::zen::{HIDE_DELAY, ZenExitAffordance};

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
