use crate::workspace::WorkspaceApp;
use gpui::{Context, Window};
use oxideterm_gpui_connections::prompts::HostKeyApproval;

impl WorkspaceApp {
    pub(in crate::workspace) fn accept_host_key_challenge(
        &mut self,
        persist: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let result = self
            .connections
            .flow
            .update(cx, |flow, cx| flow.accept_host_key(persist, &self.i18n, cx));
        match result {
            None => {}
            Some(HostKeyApproval::Tree {
                fingerprint,
                replacement,
                status,
            }) => {
                if let Some(message) = status {
                    self.set_connection_status(Some(message), cx);
                }
                self.accept_active_proxy_connect_host_key(
                    persist,
                    fingerprint,
                    replacement,
                    window,
                    cx,
                );
            }
            Some(HostKeyApproval::Retry {
                config,
                title,
                intent,
            }) => {
                self.start_ssh_preflight(config, title, intent, cx);
            }
            Some(HostKeyApproval::Continue {
                config,
                title,
                intent,
            }) => {
                self.continue_verified_ssh_flow(config, title, intent, window, cx);
            }
            Some(HostKeyApproval::Report(message)) => self.set_connection_status(Some(message), cx),
        }
    }

    pub(in crate::workspace) fn cancel_host_key_challenge(&mut self, cx: &mut Context<Self>) {
        let delay = oxideterm_gpui_ui::motion::duration(
            &self.tokens,
            oxideterm_gpui_ui::motion::MotionDuration::Overlay,
        );
        let (intent, inline) = self
            .connections
            .flow
            .update(cx, |flow, cx| flow.start_host_key_cancellation(delay, cx));
        if let Some(intent) = intent {
            self.fail_public_mcp_mosh_open_for_intent(
                &intent,
                "The host-key confirmation was cancelled",
            );
        }
        if let Some(inline) = inline {
            self.cancel_active_proxy_connect_run(cx);
            if !inline {
                self.set_connection_status(None, cx);
            }
            cx.notify();
        }
    }
}
