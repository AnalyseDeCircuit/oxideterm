use super::*;

impl WorkspaceApp {
    pub(in crate::workspace) fn handle_host_tools_window_modal_key(
        &mut self,
        modal: HostToolsWindowModalSnapshot,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        match modal {
            HostToolsWindowModalSnapshot::ProcessConfirm(_) => {
                self.handle_host_process_confirm_key(event, cx)
            }
            HostToolsWindowModalSnapshot::DockerConfirm(_) => {
                self.handle_host_docker_confirm_key(event, cx)
            }
            HostToolsWindowModalSnapshot::DockerLogs => {
                if self.handle_host_log_search_key(event, cx) {
                    return true;
                }
                if event.keystroke.key.as_str() == "escape" {
                    self.host_tools.update(cx, |host_tools, cx| {
                        host_tools.dismiss_docker_logs_dialog(cx);
                    });
                }
                true
            }
            HostToolsWindowModalSnapshot::ServiceConfirm(_) => {
                self.handle_host_service_confirm_key(event, cx)
            }
            HostToolsWindowModalSnapshot::ServiceLogs => {
                if self.handle_host_log_search_key(event, cx) {
                    return true;
                }
                if event.keystroke.key.as_str() == "escape" {
                    self.host_tools.update(cx, |host_tools, cx| {
                        host_tools.dismiss_service_logs_dialog(cx);
                    });
                }
                true
            }
            HostToolsWindowModalSnapshot::TmuxConfirm(_) => {
                self.handle_host_tmux_confirm_key(event, cx)
            }
            HostToolsWindowModalSnapshot::TmuxInput => {
                self.handle_host_tmux_input_dialog_key(event, cx)
            }
            HostToolsWindowModalSnapshot::MuxConfirm(_) => {
                self.handle_host_mux_confirm_key(event, cx)
            }
            HostToolsWindowModalSnapshot::MuxInput => {
                self.handle_host_mux_input_dialog_key(event, window, cx)
            }
            HostToolsWindowModalSnapshot::MuxRead => self.handle_host_mux_read_key(event, cx),
            HostToolsWindowModalSnapshot::ScheduleConfirm(_) => {
                self.handle_host_schedule_confirm_key(event, cx)
            }
            HostToolsWindowModalSnapshot::PackageConfirm(_) => {
                self.handle_host_package_confirm_key(event, window, cx)
            }
            HostToolsWindowModalSnapshot::ScheduleLogs => {
                if self.handle_host_log_search_key(event, cx) {
                    return true;
                }
                if event.keystroke.key.as_str() == "escape" {
                    self.host_tools.update(cx, |host_tools, cx| {
                        host_tools.dismiss_schedule_logs_dialog(cx);
                    });
                }
                true
            }
        }
    }
}
