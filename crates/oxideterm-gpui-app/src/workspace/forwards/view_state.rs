// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use super::*;
use oxideterm_editor_core::utf16::replace_utf16;

impl ForwardingWorkspaceEntity {
    pub(in crate::workspace) fn view(&self) -> &ForwardsViewState {
        &self.view
    }

    pub(super) fn clear_error(&mut self) {
        self.view.error = None;
    }

    pub(super) fn set_error(&mut self, error: String) {
        self.view.error = Some(error);
    }

    pub(super) fn begin_operation(&mut self) {
        self.view.pending = true;
        self.view.error = None;
    }

    pub(super) fn finish_operation(&mut self) {
        self.view.pending = false;
    }

    pub(super) fn reset_completed_operation_form(&mut self) {
        self.view.error = None;
        self.view.skip_health_check = false;
        self.view.focused_input = None;
    }

    pub(super) fn open_create_form(&mut self) {
        self.view.show_new_form = true;
        self.view.new_form_presence.reopen();
        self.view.error = None;
    }

    pub(super) fn open_edit_form(&mut self, rule: ForwardRule) {
        self.view.edit_bind_address = rule.bind_address.clone();
        self.view.edit_bind_port = rule.bind_port.to_string();
        self.view.edit_target_host = rule.target_host.clone();
        self.view.edit_target_port = rule.target_port.to_string();
        self.view.editing_forward = Some(rule);
        self.view.edit_form_presence.reopen();
        self.view.error = None;
        self.view.focused_input = None;
    }

    pub(super) fn begin_create_form_exit(
        &mut self,
        delay: Duration,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(generation) = self.view.new_form_presence.begin_exit() else {
            return false;
        };
        self.view.focused_input = None;
        self.schedule_form_exit(generation, true, delay, cx);
        true
    }

    pub(super) fn begin_edit_form_exit(&mut self, delay: Duration, cx: &mut Context<Self>) -> bool {
        let Some(generation) = self.view.edit_form_presence.begin_exit() else {
            return false;
        };
        self.view.focused_input = None;
        self.schedule_form_exit(generation, false, delay, cx);
        true
    }

    fn schedule_form_exit(
        &mut self,
        generation: u64,
        create_form: bool,
        delay: Duration,
        cx: &mut Context<Self>,
    ) {
        if delay.is_zero() {
            if self.finish_form_exit(generation, create_form) {
                cx.notify();
            }
            return;
        }
        // The animation belongs to the forwarding Entity so closing the form
        // never schedules a reverse dependency through WorkspaceApp.
        cx.spawn(async move |entity, cx| {
            Timer::after(delay).await;
            let _ = entity.update(cx, |entity, cx| {
                if entity.finish_form_exit(generation, create_form) {
                    cx.notify();
                }
            });
        })
        .detach();
        cx.notify();
    }

    fn finish_form_exit(&mut self, generation: u64, create_form: bool) -> bool {
        let presence = if create_form {
            &mut self.view.new_form_presence
        } else {
            &mut self.view.edit_form_presence
        };
        if !presence.finish_exit(generation) {
            return false;
        }
        presence.reopen();
        if create_form {
            self.view.show_new_form = false;
        } else {
            self.view.editing_forward = None;
        }
        true
    }

    pub(super) fn select_forward_type(&mut self, forward_type: ForwardType) {
        self.view.forward_type = forward_type;
        self.view.error = None;
        if forward_type == ForwardType::Dynamic {
            self.view.skip_health_check = false;
        }
    }

    pub(super) fn toggle_skip_health_check(&mut self) {
        self.view.skip_health_check = !self.view.skip_health_check;
        self.view.error = None;
    }

    pub(super) fn request_delete(&mut self, rule: ForwardRule) {
        self.view.pending_delete_forward = Some(rule);
        self.view.error = None;
    }

    pub(super) fn clear_pending_delete(&mut self) {
        self.view.pending_delete_forward = None;
    }

    pub(super) fn focus_input(&mut self, input: ForwardInput) {
        self.view.focused_input = Some(input);
    }

    pub(in crate::workspace) fn clear_input_focus(&mut self) -> bool {
        self.view.focused_input.take().is_some()
    }

    pub(super) fn input_value(&self, input: ForwardInput) -> &str {
        match input {
            ForwardInput::CreateBindAddress => &self.view.bind_address,
            ForwardInput::CreateBindPort => &self.view.bind_port,
            ForwardInput::CreateTargetHost => &self.view.target_host,
            ForwardInput::CreateTargetPort => &self.view.target_port,
            ForwardInput::EditBindAddress => &self.view.edit_bind_address,
            ForwardInput::EditBindPort => &self.view.edit_bind_port,
            ForwardInput::EditTargetHost => &self.view.edit_target_host,
            ForwardInput::EditTargetPort => &self.view.edit_target_port,
        }
    }

    pub(in crate::workspace) fn replace_input_text(
        &mut self,
        input: ForwardInput,
        replacement_range: Option<std::ops::Range<usize>>,
        text: &str,
    ) {
        replace_utf16(self.input_value_mut(input), replacement_range, text);
        self.view.error = None;
    }

    pub(super) fn backspace_input(&mut self, input: ForwardInput) -> bool {
        self.input_value_mut(input).pop().is_some() || self.view.error.take().is_some()
    }

    fn input_value_mut(&mut self, input: ForwardInput) -> &mut String {
        match input {
            ForwardInput::CreateBindAddress => &mut self.view.bind_address,
            ForwardInput::CreateBindPort => &mut self.view.bind_port,
            ForwardInput::CreateTargetHost => &mut self.view.target_host,
            ForwardInput::CreateTargetPort => &mut self.view.target_port,
            ForwardInput::EditBindAddress => &mut self.view.edit_bind_address,
            ForwardInput::EditBindPort => &mut self.view.edit_bind_port,
            ForwardInput::EditTargetHost => &mut self.view.edit_target_host,
            ForwardInput::EditTargetPort => &mut self.view.edit_target_port,
        }
    }

    pub(super) fn mark_stats_refreshed_if_due(&mut self, interval: Duration) -> bool {
        let due = self
            .view
            .last_stats_refresh
            .is_none_or(|last| last.elapsed() >= interval);
        if due {
            self.view.last_stats_refresh = Some(Instant::now());
        }
        due
    }

    pub(super) fn mark_forward_copied(
        &mut self,
        forward_id: String,
        delay: Duration,
        cx: &mut Context<Self>,
    ) {
        self.view.copied_forward_id = Some(forward_id.clone());
        cx.spawn(async move |entity, cx| {
            Timer::after(delay).await;
            let _ = entity.update(cx, |entity, cx| {
                if entity.view.copied_forward_id.as_deref() == Some(&forward_id) {
                    entity.view.copied_forward_id = None;
                    cx.notify();
                }
            });
        })
        .detach();
        cx.notify();
    }

    pub(super) fn sync_active_port_detection(&mut self, node_id: &NodeId) {
        let Some(state) = self.port_detection_by_node.get(node_id) else {
            self.view.detected_ports.clear();
            self.view.new_ports.clear();
            self.view.has_scanned_ports = false;
            self.view.port_scan_pending = false;
            self.view.port_scan_error = None;
            self.view.last_port_scan_started = None;
            return;
        };
        self.view.detected_ports.clone_from(&state.detected_ports);
        self.view.new_ports.clone_from(&state.new_ports);
        self.view.has_scanned_ports = state.has_scanned_ports;
        self.view.port_scan_pending = state.port_scan_pending;
        self.view.port_scan_error.clone_from(&state.port_scan_error);
        self.view.last_port_scan_started = state.last_port_scan_started;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_entity() -> ForwardingWorkspaceEntity {
        ForwardingWorkspaceEntity::test_fixture()
    }

    #[test]
    fn operation_and_input_transitions_are_entity_owned() {
        let mut entity = test_entity();
        entity.set_error("old error".to_string());

        entity.begin_operation();
        entity.focus_input(ForwardInput::CreateBindPort);
        entity.replace_input_text(ForwardInput::CreateBindPort, None, "8080");

        assert!(entity.view().pending);
        assert!(entity.view().error.is_none());
        assert_eq!(entity.input_value(ForwardInput::CreateBindPort), "8080");
        assert_eq!(
            entity.view().focused_input,
            Some(ForwardInput::CreateBindPort)
        );

        assert!(entity.backspace_input(ForwardInput::CreateBindPort));
        entity.finish_operation();
        assert!(!entity.view().pending);
        assert_eq!(entity.input_value(ForwardInput::CreateBindPort), "808");
    }

    #[test]
    fn active_detection_projection_and_dismissal_share_one_owner() {
        let mut entity = test_entity();
        let node_id = NodeId::new("forward-view");
        let detected_port = DetectedPort {
            port: 8080,
            bind_addr: "127.0.0.1".to_string(),
            process_name: None,
            pid: None,
        };
        entity.port_detection_by_node.insert(
            node_id.clone(),
            PortDetectionViewState {
                detected_ports: vec![detected_port.clone()],
                new_ports: vec![detected_port],
                has_scanned_ports: true,
                ..PortDetectionViewState::default()
            },
        );

        entity.sync_active_port_detection(&node_id);
        assert!(entity.view().has_scanned_ports);
        assert_eq!(entity.view().new_ports.len(), 1);

        entity.dismiss_detected_port(&node_id, 8080);
        assert!(entity.view().new_ports.is_empty());
        assert!(
            entity
                .port_detection_by_node
                .get(&node_id)
                .is_some_and(|state| state.new_ports.is_empty())
        );
    }
}
