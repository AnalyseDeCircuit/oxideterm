use super::*;

#[cfg(test)]
const TERMINAL_SETTINGS_SWITCHER_ID: &str = "terminal-settings-page-switcher";
#[cfg(test)]
const AI_SETTINGS_SWITCHER_ID: &str = "ai-settings-page-switcher";
pub(in crate::workspace) use oxideterm_gpui_cloud_sync::panel::CLOUD_SYNC_SWITCHER_ID;
pub(in crate::workspace) const PLUGIN_MANAGER_SWITCHER_ID: &str = "plugin-manager-tab-bar";
pub(in crate::workspace) const CONNECTION_RUNTIME_SWITCHER_ID: &str = "connection-runtime-tab-bar";

#[cfg(test)]
const SETTINGS_NAVIGATION_ID: &str = "settings-navigation";
#[cfg(test)]
const NEW_CONNECTION_AUTH_SELECTOR_ID: &str = "new-connection-auth-selector";
pub(in crate::workspace) const VERSION_MIGRATION_MOTION_SWITCHER_ID: &str =
    "version-migration-motion-switcher";

pub(super) use oxideterm_gpui_ui::motion::UserSegmentedControlMotionState;

impl WorkspaceApp {
    pub(in crate::workspace) fn begin_user_segmented_control_transition(
        &mut self,
        control_id: &'static str,
        target_index: usize,
        cx: &mut Context<Self>,
    ) {
        self.begin_user_segmented_control_transition_with_vertical_offset(
            control_id,
            target_index,
            None,
            cx,
        );
    }

    pub(in crate::workspace) fn begin_user_segmented_control_transition_from(
        &mut self,
        control_id: &'static str,
        previous_index: usize,
        target_index: usize,
        cx: &mut Context<Self>,
    ) {
        self.begin_user_segmented_control_transition_with_history(
            control_id,
            Some(previous_index),
            target_index,
            None,
            cx,
        );
    }

    pub(in crate::workspace) fn begin_user_segmented_control_transition_with_vertical_offset(
        &mut self,
        control_id: &'static str,
        target_index: usize,
        vertical_offset_y: Option<f32>,
        cx: &mut Context<Self>,
    ) {
        self.begin_user_segmented_control_transition_with_history(
            control_id,
            None,
            target_index,
            vertical_offset_y,
            cx,
        );
    }

    fn begin_user_segmented_control_transition_with_history(
        &mut self,
        control_id: &'static str,
        previous_index: Option<usize>,
        target_index: usize,
        vertical_offset_y: Option<f32>,
        cx: &mut Context<Self>,
    ) {
        let Some(motion) = oxideterm_gpui_ui::segmented_control_motion(&self.tokens) else {
            self.segmented_control_user_motion.clear(control_id);
            return;
        };
        let generation = if previous_index.is_some() {
            self.segmented_control_user_motion
                .begin_with_previous_index(
                    control_id,
                    previous_index,
                    target_index,
                    vertical_offset_y,
                )
        } else {
            self.segmented_control_user_motion
                .begin_with_vertical_offset(control_id, target_index, vertical_offset_y)
        };
        // User intent outlives a virtual-list row only for the real transition,
        // then expires so remounts and programmatic navigation render settled.
        cx.spawn(async move |weak, cx| {
            Timer::after(motion.duration).await;
            let _ = weak.update(cx, |this, cx| {
                if this
                    .segmented_control_user_motion
                    .finish(control_id, generation)
                {
                    cx.notify();
                }
            });
        })
        .detach();
    }

    pub(in crate::workspace) fn segmented_control_user_transition_active(
        &self,
        control_id: &'static str,
        active_index: usize,
    ) -> bool {
        self.segmented_control_user_motion
            .is_active_for(control_id, active_index)
    }

    pub(in crate::workspace) fn segmented_control_user_previous_index(
        &self,
        control_id: &'static str,
        active_index: usize,
    ) -> Option<usize> {
        self.segmented_control_user_motion
            .previous_index_for(control_id, active_index)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct SelectionRows {
        motions: UserSegmentedControlMotionState,
        index: usize,
        tokens: ThemeTokens,
    }

    impl Render for SelectionRows {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            div()
                .w(px(200.0))
                .flex()
                .flex_col()
                .children((0..4).map(|index| {
                    div().id(index).h(px(40.0)).w_full().relative().when(
                        index == self.index,
                        |row| {
                            row.child(
                                self.motions.render_vertical(
                                    SETTINGS_NAVIGATION_ID,
                                    index,
                                    &self.tokens,
                                    div()
                                        .absolute()
                                        .inset_0()
                                        .debug_selector(|| "row-selection".into()),
                                ),
                            )
                        },
                    )
                }))
        }
    }

    #[gpui::test]
    fn vertical_selection_preserves_visual_position_across_row_reparenting(
        cx: &mut gpui::TestAppContext,
    ) {
        let (view, cx) = cx.add_window_view(|_, _| SelectionRows {
            motions: Default::default(),
            index: 0,
            tokens: oxideterm_theme::default_tokens(),
        });
        cx.simulate_resize(gpui::size(px(300.0), px(300.0)));
        cx.update(|window, cx| window.draw(cx).clear(cx));
        view.update(cx, |view, cx| {
            view.index = 3;
            view.motions
                .begin_with_vertical_offset(SETTINGS_NAVIGATION_ID, 3, Some(-120.0));
            cx.notify();
        });
        cx.update(|window, cx| window.draw(cx).clear(cx));
        cx.executor().advance_clock(Duration::from_millis(50));
        cx.update(|window, cx| window.draw(cx).clear(cx));
        let before = cx.debug_bounds("row-selection").unwrap().top();
        assert!((f32::from(before) - 71.3).abs() <= 0.6);
        view.update(cx, |view, cx| {
            view.index = 1;
            view.motions
                .begin_with_vertical_offset(SETTINGS_NAVIGATION_ID, 1, Some(80.0));
            cx.notify();
        });
        cx.update(|window, cx| window.draw(cx).clear(cx));
        assert_eq!(cx.debug_bounds("row-selection").unwrap().top(), before);
        cx.executor().advance_clock(Duration::from_millis(220));
        cx.update(|window, cx| window.draw(cx).clear(cx));
        assert_eq!(cx.debug_bounds("row-selection").unwrap().top(), px(40.0));
        assert_eq!(
            cx.debug_bounds("row-selection").unwrap().size.height,
            px(40.0)
        );
        for profile in [
            oxideterm_theme::UiMotionProfile::Reduced,
            oxideterm_theme::UiMotionProfile::Off,
        ] {
            view.update(cx, |view, cx| {
                let offset = (view.index as f32 - 3.0) * 40.0;
                view.index = 3;
                view.motions
                    .begin_with_vertical_offset(SETTINGS_NAVIGATION_ID, 3, Some(offset));
                cx.notify();
            });
            cx.update(|window, cx| window.draw(cx).clear(cx));
            cx.executor().advance_clock(Duration::from_millis(25));
            cx.update(|window, cx| window.draw(cx).clear(cx));
            view.update(cx, |view, cx| {
                view.tokens.apply_motion(profile);
                cx.notify();
            });
            cx.update(|window, cx| window.draw(cx).clear(cx));
            assert_eq!(cx.debug_bounds("row-selection").unwrap().top(), px(120.0));
            view.update(cx, |view, cx| {
                view.tokens
                    .apply_motion(oxideterm_theme::UiMotionProfile::Normal);
                view.index = 0;
                view.motions
                    .begin_with_vertical_offset(SETTINGS_NAVIGATION_ID, 0, Some(120.0));
                cx.notify();
            });
            cx.update(|window, cx| window.draw(cx).clear(cx));
            assert_eq!(
                cx.debug_bounds("row-selection").unwrap().top(),
                px(120.0),
                "{profile:?} must clear old travel"
            );
            cx.executor().advance_clock(Duration::from_millis(220));
            cx.update(|window, cx| window.draw(cx).clear(cx));
            assert_eq!(cx.debug_bounds("row-selection").unwrap().top(), px(0.0));
        }
    }

    #[test]
    fn stale_or_cleared_generations_cannot_finish_newer_transitions() {
        let mut state = UserSegmentedControlMotionState::default();
        let first = state.begin_with_vertical_offset(TERMINAL_SETTINGS_SWITCHER_ID, 1, None);
        let second = state.begin_with_vertical_offset(TERMINAL_SETTINGS_SWITCHER_ID, 2, None);

        assert!(!state.finish(TERMINAL_SETTINGS_SWITCHER_ID, first));
        assert!(state.is_active_for(TERMINAL_SETTINGS_SWITCHER_ID, 2));
        assert!(state.finish(TERMINAL_SETTINGS_SWITCHER_ID, second));
        assert!(!state.is_active_for(TERMINAL_SETTINGS_SWITCHER_ID, 2));

        let cleared_generation = state.begin_with_vertical_offset(AI_SETTINGS_SWITCHER_ID, 1, None);

        state.clear(AI_SETTINGS_SWITCHER_ID);
        let replacement_generation =
            state.begin_with_vertical_offset(AI_SETTINGS_SWITCHER_ID, 2, None);

        assert_ne!(cleared_generation, replacement_generation);
        assert!(!state.finish(AI_SETTINGS_SWITCHER_ID, cleared_generation));
        assert!(state.is_active_for(AI_SETTINGS_SWITCHER_ID, 2));
    }

    #[test]
    fn transition_metadata_survives_until_replacement_or_completion() {
        let mut state = UserSegmentedControlMotionState::default();
        state.begin_with_vertical_offset(SETTINGS_NAVIGATION_ID, 3, Some(-52.0));
        let latest_generation =
            state.begin_with_vertical_offset(SETTINGS_NAVIGATION_ID, 7, Some(91.0));

        assert_eq!(
            state.transition_for(SETTINGS_NAVIGATION_ID, 7),
            Some((latest_generation, Some(91.0)))
        );
        assert_eq!(state.transition_for(SETTINGS_NAVIGATION_ID, 3), None);

        let generation =
            state.begin_with_previous_index(NEW_CONNECTION_AUTH_SELECTOR_ID, Some(0), 2, None);

        assert_eq!(
            state.previous_index_for(NEW_CONNECTION_AUTH_SELECTOR_ID, 2),
            Some(0)
        );
        assert!(state.finish(NEW_CONNECTION_AUTH_SELECTOR_ID, generation));
        assert_eq!(
            state.previous_index_for(NEW_CONNECTION_AUTH_SELECTOR_ID, 2),
            None
        );
    }
}
