use super::WorkspaceApp;
use gpui::{Context, KeyDownEvent, Window};
use oxideterm_gpui_connections::views::{FormKeyAction, select_anchor_id};

impl WorkspaceApp {
    pub(in crate::workspace) fn handle_new_connection_key(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let caret_visible = self.input_caret.visible();
        let tunnel_fields = self.ssh_tunnel_input_fields();
        let (action, show_caret) = self.connections.flow.update(cx, |flow, cx| {
            flow.handle_form_key(event, caret_visible, tunnel_fields.as_deref(), cx)
        });
        if show_caret {
            self.show_active_input_caret(cx);
        }
        match action {
            FormKeyAction::NotHandled => return false,
            FormKeyAction::Handled => {}
            FormKeyAction::CloseForm if self.ssh_tunnels.form.is_some() => {
                self.close_ssh_tunnel_form(window, cx)
            }
            FormKeyAction::CloseForm => self.close_new_connection_form(window, cx),
            FormKeyAction::Submit if self.ssh_tunnels.form.is_some() => {
                self.save_ssh_tunnel(true, window, cx)
            }
            FormKeyAction::Submit => self.submit_new_connection_form(window, cx),
            FormKeyAction::CloseJumpForm | FormKeyAction::AddJumpServer => {
                self.connections.finish_jump_form(
                    action == FormKeyAction::AddJumpServer,
                    &self.tokens,
                    cx,
                );
                self.ime_marked_text = None;
            }
            FormKeyAction::Paste => self.paste_into_new_connection_field(cx),
        }
        true
    }

    pub(in crate::workspace) fn paste_into_new_connection_field(&mut self, cx: &mut Context<Self>) {
        if self
            .connections
            .flow
            .update(cx, |flow, cx| flow.paste_form_input(cx))
        {
            self.show_active_input_caret(cx);
        }
    }

    pub(in crate::workspace) fn close_new_connection_select(&mut self, cx: &mut Context<Self>) {
        self.update_connection_form_state(cx, |state| state.close_select());
    }

    pub(in crate::workspace) fn new_connection_select_anchor_id(
        select: super::NewConnectionSelect,
    ) -> oxideterm_gpui_ui::select::SelectAnchorId {
        select_anchor_id(select)
    }

    pub(in crate::workspace) fn connection_form_group_is_ungrouped(&self, group: &str) -> bool {
        oxideterm_gpui_connections::conversion::connection_group_is_ungrouped(group, &self.i18n)
    }

    pub(in crate::workspace) fn refresh_serial_ports(&mut self, cx: &mut Context<Self>) {
        self.connections.refresh_serial_ports(cx);
    }

    pub(in crate::workspace) fn refresh_ble_adapters(&mut self, cx: &mut Context<Self>) {
        self.connections.refresh_ble_adapters(cx);
    }

    pub(in crate::workspace) fn refresh_container_engines(&mut self, cx: &mut Context<Self>) {
        self.connections.refresh_container_engines(cx);
    }
}
