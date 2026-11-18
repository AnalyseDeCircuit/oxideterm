use super::*;

impl WorkspaceApp {
    pub(super) fn session_manager_display_items(&self, cx: &App) -> Vec<SessionManagerDisplayItem> {
        self.connections
            .session_manager
            .read(cx)
            .display_catalog(&self.connection_store, self.saved_ssh_tunnels())
    }

    pub(in crate::workspace) fn open_session_manager_target_with_sftp(
        &mut self,
        target: SessionManagerOpenTarget,
        open_sftp_on_connect_override: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match target {
            SessionManagerOpenTarget::Tunnel(id) => self.open_ssh_tunnel(&id, false, window, cx),
            SessionManagerOpenTarget::Plugin(id) => self.open_plugin_connection(&id, cx),
            SessionManagerOpenTarget::Ftp(id) => self.open_saved_ftp_profile(&id, window, cx),
            SessionManagerOpenTarget::Connection(connection_id) => self
                .open_saved_connection_with_sftp(
                    &connection_id,
                    open_sftp_on_connect_override,
                    window,
                    cx,
                ),
            SessionManagerOpenTarget::SshConfig(alias) => {
                self.open_ssh_config_alias_from_palette(alias, window, cx)
            }
            SessionManagerOpenTarget::Serial(profile_id) => {
                self.open_saved_serial_profile(&profile_id, window, cx)
            }
            SessionManagerOpenTarget::Ble(profile_id) => {
                self.open_saved_ble_profile(&profile_id, window, cx)
            }
            SessionManagerOpenTarget::LocalTerminal(profile_id) => {
                self.open_saved_local_terminal_profile(&profile_id, window, cx)
            }
            SessionManagerOpenTarget::Telnet(profile_id) => {
                self.open_saved_telnet_profile(&profile_id, window, cx)
            }
            SessionManagerOpenTarget::Mosh(profile_id) => {
                self.open_saved_mosh_profile(&profile_id, window, cx)
            }
            SessionManagerOpenTarget::StandaloneSftp(profile_id) => {
                self.open_saved_standalone_sftp_profile(&profile_id, window, cx)
            }
            SessionManagerOpenTarget::RemoteDesktop(profile_id) => {
                self.open_saved_remote_desktop_profile(&profile_id, window, cx)
            }
        }
    }
}
