use std::{cell::Cell, collections::HashMap, rc::Rc};

use gpui::{Animation, AnimationExt, AnyElement, IntoElement, Styled, px};
use oxideterm_theme::ThemeTokens;

#[cfg(test)]
mod tests {
    use super::UserSegmentedControlMotionState;

    #[test]
    fn transitions_are_bounded_per_control_and_target() {
        let mut state = UserSegmentedControlMotionState::default();
        state.begin_with_vertical_offset("first", 1, None);
        assert!(state.is_active_for("first", 1));
        assert!(!state.is_active_for("first", 2));

        let mut latest = 0;
        for target_index in 0..1_000 {
            latest = state.begin_with_vertical_offset("second", target_index, None);
        }

        assert_eq!(state.active_transitions.len(), 2);
        assert!(state.is_active_for("first", 1));
        assert_eq!(state.transition_for("second", 999), Some((latest, None)));
        state.clear("first");
        assert_eq!(state.active_transitions.len(), 1);
    }
}

#[derive(Default)]
pub struct UserSegmentedControlMotionState {
    next_generation: u64,
    active_transitions: HashMap<&'static str, ActiveUserTransition>,
}

#[derive(Clone)]
struct ActiveUserTransition {
    generation: u64,
    target_index: usize,
    previous_index: Option<usize>,
    vertical_offset_y: Option<f32>,
    vertical_from: gpui::SpringState,
    vertical_position: Rc<Cell<gpui::SpringState>>,
}

impl UserSegmentedControlMotionState {
    pub fn begin_with_vertical_offset(
        &mut self,
        control_id: &'static str,
        target_index: usize,
        vertical_offset_y: Option<f32>,
    ) -> u64 {
        self.begin_with_previous_index(control_id, None, target_index, vertical_offset_y)
    }

    pub fn begin_with_previous_index(
        &mut self,
        control_id: &'static str,
        previous_index: Option<usize>,
        target_index: usize,
        vertical_offset_y: Option<f32>,
    ) -> u64 {
        // One monotonic counter prevents a cleared control from reusing a
        // generation while its older completion task is still pending.
        self.next_generation = self.next_generation.wrapping_add(1).max(1);
        let generation = self.next_generation;
        let previous = self
            .active_transitions
            .get(control_id)
            .map(|transition| transition.vertical_position.get());
        let vertical_from = gpui::SpringState {
            position: vertical_offset_y.unwrap_or(0.0)
                + previous.map_or(0.0, |state| state.position),
            velocity: previous.map_or(0.0, |state| state.velocity),
        };
        self.active_transitions.insert(
            control_id,
            ActiveUserTransition {
                generation,
                target_index,
                previous_index,
                vertical_offset_y,
                vertical_from,
                vertical_position: Rc::new(Cell::new(vertical_from)),
            },
        );
        generation
    }

    pub fn finish(&mut self, control_id: &'static str, generation: u64) -> bool {
        if self
            .active_transitions
            .get(control_id)
            .map(|transition| transition.generation)
            != Some(generation)
        {
            return false;
        }
        self.active_transitions.remove(control_id);
        true
    }

    pub fn clear(&mut self, control_id: &'static str) {
        self.active_transitions.remove(control_id);
    }

    pub fn is_active_for(&self, control_id: &'static str, active_index: usize) -> bool {
        self.active_transitions
            .get(control_id)
            .is_some_and(|transition| transition.target_index == active_index)
    }

    pub fn transition_for(
        &self,
        control_id: &'static str,
        active_index: usize,
    ) -> Option<(u64, Option<f32>)> {
        self.active_transitions
            .get(control_id)
            .filter(|transition| transition.target_index == active_index)
            .map(|transition| (transition.generation, transition.vertical_offset_y))
    }

    pub fn previous_index_for(
        &self,
        control_id: &'static str,
        active_index: usize,
    ) -> Option<usize> {
        self.active_transitions
            .get(control_id)
            .filter(|transition| transition.target_index == active_index)
            .and_then(|transition| transition.previous_index)
    }
}

impl UserSegmentedControlMotionState {
    pub fn render_vertical(
        &self,
        control_id: &'static str,
        active_index: usize,
        tokens: &ThemeTokens,
        surface: gpui::Div,
    ) -> AnyElement {
        let Some(entry) = self
            .active_transitions
            .get(control_id)
            .filter(|entry| entry.target_index == active_index)
        else {
            return surface.into_any_element();
        };
        let Some(motion) = crate::segmented_control_motion(tokens) else {
            entry.vertical_position.set(gpui::SpringState {
                position: 0.0,
                velocity: 0.0,
            });
            return surface.into_any_element();
        };
        let id = gpui::ElementId::NamedInteger(control_id.into(), entry.generation);
        if !motion.spatial || entry.vertical_offset_y.is_none() {
            entry.vertical_position.set(gpui::SpringState {
                position: 0.0,
                velocity: 0.0,
            });
            return crate::motion::fade_in(
                tokens,
                id,
                surface,
                crate::motion::MotionDuration::Control,
            );
        }
        // Row reparenting changes the GPUI ID path. The workspace retains only
        // scalar position and velocity so the next row continues the same motion.
        let from = entry.vertical_from;
        let position = entry.vertical_position.clone();
        let config = crate::motion::spring_config(tokens, crate::motion::MotionDuration::Control);
        surface
            .with_animation(
                id,
                Animation::new(motion.duration),
                move |surface, progress| {
                    let state = if progress >= 1.0 {
                        gpui::SpringState {
                            position: 0.0,
                            velocity: 0.0,
                        }
                    } else {
                        config.step(from, 0.0, progress * motion.duration.as_secs_f32())
                    };
                    position.set(state);
                    surface.top(px(state.position)).bottom(px(-state.position))
                },
            )
            .into_any_element()
    }
}
