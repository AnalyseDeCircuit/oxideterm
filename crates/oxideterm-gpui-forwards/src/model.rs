use crate::data::{DetectedPort, ForwardRule, ForwardType, PortDetectionSnapshot};
use crate::style::*;
use gpui::{Context, Instant, ListAlignment, ListState, px};
use oxideterm_editor_core::utf16::replace_utf16;
use oxideterm_gpui_ui::virtual_list::{TauriVirtualListSpec, VirtualListSignatureCache};
use std::{cell::RefCell, time::Duration};

pub enum ForwardsInputKey {
    Unhandled,
    Handled,
    Blur,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub enum ForwardInput {
    CreateBindAddress,
    CreateBindPort,
    CreateTargetHost,
    CreateTargetPort,
    EditBindAddress,
    EditBindPort,
    EditTargetHost,
    EditTargetPort,
}

impl ForwardInput {
    pub fn anchor_key(self) -> u64 {
        match self {
            Self::CreateBindAddress => 1,
            Self::CreateBindPort => 2,
            Self::CreateTargetHost => 3,
            Self::CreateTargetPort => 4,
            Self::EditBindAddress => 5,
            Self::EditBindPort => 6,
            Self::EditTargetHost => 7,
            Self::EditTargetPort => 8,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn connection_handoff_discards_previous_detected_ports() {
        let mut state = PortDetectionViewState::default();
        let port = DetectedPort {
            port: 3000,
            bind_addr: "127.0.0.1".to_owned(),
            process_name: None,
            pid: None,
        };
        state.apply(
            Some("connection-a".to_owned()),
            Ok(PortDetectionSnapshot {
                new_ports: vec![port.clone()],
                all_ports: vec![port],
                has_scanned: true,
                ..Default::default()
            }),
        );
        assert_eq!(
            state.new_ports.iter().map(|p| p.port).collect::<Vec<_>>(),
            [3000]
        );
        state.apply(
            Some("connection-b".to_owned()),
            Ok(PortDetectionSnapshot::default()),
        );
        assert_eq!(state.connection_id.as_deref(), Some("connection-b"));
        assert!(state.new_ports.is_empty());
        assert!(state.detected_ports.is_empty());
    }
}

#[derive(Debug)]
pub(crate) struct ForwardsViewState {
    pub(crate) show_new_form: bool,
    pub(crate) new_form_presence: oxideterm_gpui_ui::motion::ExitPresence,
    pub(crate) editing_forward: Option<ForwardRule>,
    pub(crate) edit_form_presence: oxideterm_gpui_ui::motion::ExitPresence,
    pub(crate) pending_delete_forward: Option<ForwardRule>,
    pub(crate) copied_forward_id: Option<String>,
    pub(crate) forward_type: ForwardType,
    pub(crate) bind_address: String,
    pub(crate) bind_port: String,
    pub(crate) target_host: String,
    pub(crate) target_port: String,
    pub(crate) skip_health_check: bool,
    pub(crate) edit_bind_address: String,
    pub(crate) edit_bind_port: String,
    pub(crate) edit_target_host: String,
    pub(crate) edit_target_port: String,
    pub(crate) focused_input: Option<ForwardInput>,
    pub(crate) error: Option<String>,
    pub(crate) pending: bool,
}

impl Default for ForwardsViewState {
    fn default() -> Self {
        Self {
            show_new_form: false,
            new_form_presence: oxideterm_gpui_ui::motion::ExitPresence::visible(),
            editing_forward: None,
            edit_form_presence: oxideterm_gpui_ui::motion::ExitPresence::visible(),
            pending_delete_forward: None,
            copied_forward_id: None,
            forward_type: ForwardType::Local,
            bind_address: FORWARDS_DEFAULT_BIND_ADDRESS.to_string(),
            bind_port: String::new(),
            target_host: FORWARDS_DEFAULT_TARGET_HOST.to_string(),
            target_port: String::new(),
            skip_health_check: false,
            edit_bind_address: FORWARDS_DEFAULT_BIND_ADDRESS.to_string(),
            edit_bind_port: String::new(),
            edit_target_host: FORWARDS_DEFAULT_TARGET_HOST.to_string(),
            edit_target_port: String::new(),
            focused_input: None,
            error: None,
            pending: false,
        }
    }
}

pub struct ForwardsPageState {
    pub(crate) view: ForwardsViewState,
    pub(super) section_list_state: ListState,
    pub(super) section_list_cache: RefCell<VirtualListSignatureCache>,
    pub(super) table_row_list_state: ListState,
    pub(super) table_row_list_cache: RefCell<VirtualListSignatureCache>,
}
impl ForwardsPageState {
    pub fn new() -> Self {
        Self {
            view: ForwardsViewState::default(),
            section_list_state: ListState::new(
                FORWARDS_SECTION_LIST_INITIAL_ITEM_COUNT,
                ListAlignment::Top,
                TauriVirtualListSpec::new(
                    px(FORWARDS_SECTION_LIST_ESTIMATED_HEIGHT),
                    FORWARDS_SECTION_LIST_OVERSCAN,
                )
                .overdraw(),
            )
            .measure_all(),
            section_list_cache: RefCell::new(VirtualListSignatureCache::default()),
            table_row_list_state: ListState::new(
                FORWARDS_TABLE_ROW_LIST_INITIAL_ITEM_COUNT,
                ListAlignment::Top,
                TauriVirtualListSpec::new(
                    px(FORWARDS_TABLE_ROW_LIST_ESTIMATED_HEIGHT),
                    FORWARDS_TABLE_ROW_LIST_OVERSCAN,
                )
                .overdraw(),
            )
            .measure_all(),
            table_row_list_cache: RefCell::new(VirtualListSignatureCache::default()),
        }
    }
}

impl ForwardsPageState {
    pub fn pending(&self) -> bool {
        self.view.pending
    }
    pub fn focused_input(&self) -> Option<ForwardInput> {
        self.view.focused_input
    }

    pub fn take_delete(&mut self) -> Option<String> {
        self.view.pending_delete_forward.take().map(|rule| rule.id)
    }

    pub fn complete_operation(
        &mut self,
        result: Result<(), String>,
        visible: bool,
        delay: Duration,
        cx: &mut Context<Self>,
    ) {
        self.finish_operation();
        if visible {
            match result {
                Ok(()) => {
                    if self.view.show_new_form {
                        self.begin_create_form_exit(delay, cx);
                    }
                    if self.edit_form_open() {
                        self.begin_edit_form_exit(delay, cx);
                    }
                    self.reset_completed_operation_form();
                }
                Err(error) => self.set_error(error),
            }
        }
        cx.notify();
    }

    pub fn handle_input_key(
        &mut self,
        event: &gpui::KeyDownEvent,
        cx: &mut Context<Self>,
    ) -> ForwardsInputKey {
        let Some(input) = self.focused_input() else {
            return ForwardsInputKey::Unhandled;
        };
        let keystroke = &event.keystroke;
        if keystroke.modifiers.platform || keystroke.modifiers.control {
            return ForwardsInputKey::Unhandled;
        }
        match keystroke.key.as_str() {
            "escape" => {
                self.clear_input_focus();
                cx.notify();
                ForwardsInputKey::Blur
            }
            "backspace" => {
                if self.backspace_input(input) {
                    cx.notify();
                }
                ForwardsInputKey::Handled
            }
            _ => ForwardsInputKey::Unhandled,
        }
    }

    pub fn edit_form_open(&self) -> bool {
        self.view.editing_forward.is_some()
    }

    pub fn edit_form_phase(&self) -> oxideterm_gpui_ui::motion::ExitPhase {
        self.view.edit_form_presence.phase()
    }

    pub fn delete_confirm_open(&self) -> bool {
        self.view.pending_delete_forward.is_some()
    }

    pub fn clear_error(&mut self) {
        self.view.error = None;
    }

    pub fn set_error(&mut self, error: String) {
        self.view.error = Some(error);
    }

    pub fn begin_operation(&mut self) {
        self.view.pending = true;
        self.view.error = None;
    }

    pub fn finish_operation(&mut self) {
        self.view.pending = false;
    }

    pub fn reset_completed_operation_form(&mut self) {
        self.view.error = None;
        self.view.skip_health_check = false;
        self.view.focused_input = None;
    }

    pub fn open_create_form(&mut self) {
        self.view.show_new_form = true;
        self.view.new_form_presence.reopen();
        self.view.error = None;
    }

    pub fn open_edit_form(&mut self, rule: ForwardRule) {
        self.view.edit_bind_address = rule.bind_address.clone();
        self.view.edit_bind_port = rule.bind_port.to_string();
        self.view.edit_target_host = rule.target_host.clone();
        self.view.edit_target_port = rule.target_port.to_string();
        self.view.editing_forward = Some(rule);
        self.view.edit_form_presence.reopen();
        self.view.error = None;
        self.view.focused_input = None;
    }

    pub fn begin_create_form_exit(&mut self, delay: Duration, cx: &mut Context<Self>) -> bool {
        let Some(generation) = self.view.new_form_presence.begin_exit() else {
            return false;
        };
        self.view.focused_input = None;
        self.schedule_form_exit(generation, true, delay, cx);
        true
    }

    pub fn begin_edit_form_exit(&mut self, delay: Duration, cx: &mut Context<Self>) -> bool {
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
        let timer = cx.background_executor().timer(delay);
        cx.spawn(async move |entity, cx| {
            timer.await;
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

    pub fn request_delete(&mut self, rule: ForwardRule) {
        self.view.pending_delete_forward = Some(rule);
        self.view.error = None;
    }

    pub fn clear_pending_delete(&mut self) {
        self.view.pending_delete_forward = None;
    }

    pub fn focus_input(&mut self, input: ForwardInput) {
        self.view.focused_input = Some(input);
    }

    pub fn clear_input_focus(&mut self) -> bool {
        self.view.focused_input.take().is_some()
    }

    pub fn input_value(&self, input: ForwardInput) -> &str {
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

    pub fn replace_input_text(
        &mut self,
        input: ForwardInput,
        replacement_range: Option<std::ops::Range<usize>>,
        text: &str,
    ) {
        replace_utf16(self.input_value_mut(input), replacement_range, text);
        self.view.error = None;
    }

    pub(super) fn set_input_value(&mut self, input: ForwardInput, value: String) {
        *self.input_value_mut(input) = value;
        self.view.error = None;
    }

    pub fn backspace_input(&mut self, input: ForwardInput) -> bool {
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

    pub(super) fn mark_forward_copied(
        &mut self,
        forward_id: String,
        delay: Duration,
        cx: &mut Context<Self>,
    ) {
        self.view.copied_forward_id = Some(forward_id.clone());
        let timer = cx.background_executor().timer(delay);
        cx.spawn(async move |entity, cx| {
            timer.await;
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
}

#[derive(Clone, Debug, Default)]
pub struct PortDetectionViewState {
    pub connection_id: Option<String>,
    pub detected_ports: Vec<DetectedPort>,
    pub new_ports: Vec<DetectedPort>,
    pub has_scanned_ports: bool,
    pub port_scan_pending: bool,
    pub port_scan_error: Option<String>,
    pub last_port_scan_started: Option<Instant>,
}

impl PortDetectionViewState {
    pub fn apply(
        &mut self,
        connection_id: Option<String>,
        result: Result<PortDetectionSnapshot, String>,
    ) {
        self.port_scan_pending = false;
        match result {
            Ok(snapshot) => self.apply_snapshot(connection_id, snapshot),
            Err(_) => self.port_scan_error = None,
        }
    }

    pub fn apply_snapshot(
        &mut self,
        connection_id: Option<String>,
        snapshot: PortDetectionSnapshot,
    ) {
        let state = self;
        if connection_id.is_some() && state.connection_id != connection_id {
            // Detection is connection-scoped. Reconnect must discard samples
            // and dismissals associated with the previous transport.
            state.connection_id = connection_id;
            state.detected_ports.clear();
            state.new_ports.clear();
            state.has_scanned_ports = false;
            state.port_scan_error = None;
        }
        state.has_scanned_ports = snapshot.has_scanned;
        state.detected_ports = snapshot.all_ports;
        if !snapshot.new_ports.is_empty() {
            let existing = state
                .new_ports
                .iter()
                .map(|port| port.port)
                .collect::<std::collections::HashSet<_>>();
            state.new_ports.extend(
                snapshot
                    .new_ports
                    .into_iter()
                    .filter(|port| !existing.contains(&port.port)),
            );
        }
        if !snapshot.closed_ports.is_empty() {
            let closed = snapshot
                .closed_ports
                .iter()
                .map(|port| port.port)
                .collect::<std::collections::HashSet<_>>();
            state.new_ports.retain(|port| !closed.contains(&port.port));
        }
        state.port_scan_error = None;
    }
}
