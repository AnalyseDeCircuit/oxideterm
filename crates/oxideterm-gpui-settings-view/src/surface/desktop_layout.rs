use crate::{SettingsTab, TerminalSettingsPage, entity::SettingsWorkspaceEntity};
use oxideterm_connections::{
    ConnectionStore, LOCAL_SHELL_PRIVILEGE_CONNECTION_ID, SSH_COMMON_PRIVILEGE_SCOPE_ID,
};
use oxideterm_settings::{BackgroundScope, PersistedSettings, SettingsUpstreamProxyAuth};
use oxideterm_settings_model::{
    SETTINGS_SECTION_HEADER_ITEM_COUNT, SettingsDynamicSectionCounts, SettingsSectionId,
    ThemeTarget, is_custom_theme_id, settings_section_list_item_count, settings_tab_section_at,
};
use std::hash::{DefaultHasher, Hash, Hasher};

/// Inputs owned by runtime services outside the settings surface.
pub struct SettingsSectionLayout<'a> {
    pub state: &'a SettingsWorkspaceEntity,
    pub settings: &'a PersistedSettings,
    pub connections: &'a ConnectionStore,
    pub app_lock_configured: bool,
    pub shell_count: usize,
    pub mcp_clients: &'a [oxideterm_public_mcp::ClientProjection],
    pub mcp_approvals: &'a [oxideterm_public_mcp::ApprovalProjection],
    pub mcp_credential_ready: bool,
    pub mcp_startup_error: bool,
}

impl SettingsSectionLayout<'_> {
    pub fn signatures(
        &self,
        counts: SettingsDynamicSectionCounts,
        mut extend: impl FnMut(SettingsTab, Option<SettingsSectionId>, &mut DefaultHasher),
    ) -> Vec<u64> {
        let tab = self.state.route_snapshot().active_tab;
        (0..settings_section_list_item_count(tab, counts))
            .map(|index| {
                let section = index
                    .checked_sub(SETTINGS_SECTION_HEADER_ITEM_COUNT)
                    .and_then(|index| settings_tab_section_at(tab, index, counts));
                let mut hasher = DefaultHasher::new();
                tab.hash(&mut hasher);
                index.hash(&mut hasher);
                hash_settings_section_layout(
                    self.state,
                    self.settings,
                    section,
                    self.app_lock_configured,
                    self.shell_count,
                    &mut hasher,
                );
                self.hash_page(tab, index, section, &mut hasher);
                extend(tab, section, &mut hasher);
                hasher.finish()
            })
            .collect()
    }

    fn hash_page(
        &self,
        tab: SettingsTab,
        index: usize,
        section: Option<SettingsSectionId>,
        hasher: &mut DefaultHasher,
    ) {
        let state = self.state;
        let store = self.connections;
        let settings = self.settings;
        match tab {
            SettingsTab::Network => {
                settings.network.upstream_proxy.is_some().hash(hasher);
                settings
                    .network
                    .upstream_proxy
                    .as_ref()
                    .map(|proxy| matches!(proxy.auth, SettingsUpstreamProxyAuth::Password { .. }))
                    .hash(hasher);
                settings
                    .network
                    .upstream_proxy_disclaimer_accepted
                    .hash(hasher);
                settings.network.application_proxy_mode.hash(hasher);
                settings.general.update_proxy.mode.hash(hasher);
                settings.general.update_proxy.protocol.hash(hasher);
                state.network_proxy_layout_flags().hash(hasher);
                for client in self.mcp_clients.iter() {
                    client.client_ref.as_str().hash(hasher);
                    client.label.hash(hasher);
                    client.enabled.hash(hasher);
                    client.approval_mode.hash(hasher);
                    client.tool_groups.hash(hasher);
                }
                for approval in self.mcp_approvals.iter().filter(|approval| {
                    approval.status == oxideterm_public_mcp::ApprovalStatus::Pending
                }) {
                    // A different frozen action can have a different wrapped command height.
                    approval.approval_ref.as_str().hash(hasher);
                }
                self.mcp_credential_ready.hash(hasher);
                self.mcp_startup_error.hash(hasher);
            }
            SettingsTab::Discovery if section == Some(SettingsSectionId::DiscoverySources) => {
                for source in &settings.discovery_sources {
                    source.id.hash(hasher);
                    source.name.hash(hasher);
                    source.enabled.hash(hasher);
                    source.removal_pending.hash(hasher);
                    state.discovery.feedback.get(&source.id).hash(hasher);
                }
                state.discovery.hash_controls(hasher);
            }
            SettingsTab::Connections => {
                if section == Some(SettingsSectionId::ConnectionsKnownHosts) {
                    let state = &state.known_hosts;
                    state.query.hash(hasher);
                    state.task.is_some().hash(hasher);
                    state.error.hash(hasher);
                    state
                        .snapshot
                        .as_ref()
                        .map(|snapshot| snapshot.entries.len())
                        .hash(hasher);
                    state
                        .pending_delete
                        .as_ref()
                        .map(|entry| (&entry.host, entry.port, &entry.fingerprint))
                        .hash(hasher);
                }
                store.connections().len().hash(hasher);
                store.managed_ssh_keys().len().hash(hasher);
                state.managed_key_status().is_some().hash(hasher);
                if section == Some(SettingsSectionId::ConnectionsImporters) {
                    // Importer state only changes the importer card. Invalidating
                    // earlier measured rows makes GPUI move the current scroll anchor.
                    state.connection_import_list_signature().hash(hasher);
                }
            }
            SettingsTab::Privilege => {
                store
                    .list_privilege_credentials(SSH_COMMON_PRIVILEGE_SCOPE_ID)
                    .map(|credentials| credentials.len())
                    .unwrap_or(0)
                    .hash(hasher);
                store.totp_credentials().len().hash(hasher);
                state.totp_draft.is_some().hash(hasher);
                state.totp_error.is_some().hash(hasher);
                store.connections().len().hash(hasher);
                store
                    .connections()
                    .iter()
                    .map(|connection| connection.privilege_credentials.len())
                    .sum::<usize>()
                    .hash(hasher);
                store
                    .list_privilege_credentials(LOCAL_SHELL_PRIVILEGE_CONNECTION_ID)
                    .map(|credentials| credentials.len())
                    .unwrap_or(0)
                    .hash(hasher);
                state.privilege_layout_flags().hash(hasher);
            }
            SettingsTab::Keybindings => {
                // Keep the search control mounted while scope filtering replaces tables.
                if index > 0 {
                    let keybinding_state = state;
                    format!("{:?}", keybinding_state.keybinding_scope_filter()).hash(hasher);
                    keybinding_state
                        .keybinding_search_query()
                        .trim()
                        .hash(hasher);
                }
                settings.keybindings.overrides.len().hash(hasher);
            }
            _ => {}
        }
    }
}

/// The settings surface owns its row height inputs; the window supplies only
/// counts and flags from features that have independent runtime owners.
fn hash_settings_section_layout(
    state: &SettingsWorkspaceEntity,
    settings: &PersistedSettings,
    section: Option<SettingsSectionId>,
    app_lock_configured: bool,
    shell_count: usize,
    hasher: &mut DefaultHasher,
) {
    let route = state.route_snapshot();
    let dark = state.appearance_edit_dark.unwrap_or(false);
    match route.active_tab {
        SettingsTab::General => {
            let launch_at_login = state.launch_at_login_snapshot();
            launch_at_login.enabled.hash(hasher);
            launch_at_login.pending.hash(hasher);
            launch_at_login.error.hash(hasher);
            settings.general.minimize_to_tray_on_close.hash(hasher);
            let cli = state.cli_companion_snapshot();
            cli.loading.hash(hasher);
            cli.error.is_some().hash(hasher);
            cli.status.hash(hasher);
            if section == Some(SettingsSectionId::GeneralAppLock) {
                // Only the application-lock card changes height when its
                // configured action set switches between one and two buttons.
                app_lock_configured.hash(hasher);
                settings.sidebar_ui.show_app_lock_icon.hash(hasher);
            }
        }
        SettingsTab::Terminal => {
            format!("{:?}", route.terminal_page).hash(hasher);
            if section == Some(SettingsSectionId::TerminalCommandBarFocusHandoff) {
                // Selected chips can change width and wrap this card, but
                // they must not invalidate measurements for every terminal row.
                settings
                    .terminal
                    .command_bar
                    .focus_handoff_commands
                    .hash(hasher);
            }
            if route.terminal_page == TerminalSettingsPage::Local {
                settings.local_terminal.oh_my_posh_enabled.hash(hasher);
                settings.local_terminal.default_shell_id.hash(hasher);
                shell_count.hash(hasher);
            }
        }
        SettingsTab::Sftp => {
            settings.sftp.speed_limit_enabled.hash(hasher);
        }
        SettingsTab::Appearance => {
            if matches!(
                section,
                Some(SettingsSectionId::AppearanceTheme | SettingsSectionId::AppearanceBackground)
            ) {
                settings.appearance.follow_system_appearance.hash(hasher);
                settings.appearance.manual_scheme.hash(hasher);
                state.appearance_edit_dark.hash(hasher);
                let style = settings.terminal.background_for_scheme(Some(dark));
                (style.enabled && style.day_cycle).hash(hasher);
                if style.day_cycle {
                    (style.image.is_some() || style.effect.is_some()).hash(hasher);
                    (state.theme_preview_page as u8).hash(hasher);
                    style.enabled_tabs.hash(hasher);
                    (style.scope == BackgroundScope::Window).hash(hasher);
                }
            }
            if section == Some(SettingsSectionId::AppearanceTheme) {
                for target in [ThemeTarget::Application, ThemeTarget::Terminal] {
                    is_custom_theme_id(target.scheme_id(settings, dark)).hash(hasher);
                }
            } else if section == Some(SettingsSectionId::AppearanceBackground) {
                // Source, effect capabilities, and collapsible background sub-panels add or
                // hide rows; paint-only adjustments keep scroll anchoring stable.
                let settings_entity = state;
                settings_entity.background_effects_expanded.hash(hasher);
                settings_entity
                    .background_interactions_expanded
                    .hash(hasher);
                let style = settings.terminal.background_for_scheme(Some(dark));
                style.image.is_some().hash(hasher);
                style.parallax.hash(hasher);
                style.camera.is_some().hash(hasher);
                style.day_cycle.hash(hasher);
                style
                    .effect
                    .as_ref()
                    .map(|effect| (effect.kind as u8, effect.has_motion()))
                    .hash(hasher);
                (style.scope == BackgroundScope::Content).hash(hasher);
                settings_entity
                    .background_media_has_animation(style.image.as_deref())
                    .hash(hasher);
            }
        }
        SettingsTab::Help => {
            settings.general.update_channel.hash(hasher);
        }
        SettingsTab::Portable => {
            let portable = state.portable_status_snapshot();
            portable.refresh_pending.hash(hasher);
            portable.error.is_some().hash(hasher);
            portable.exportable_secret_count.hash(hasher);
            if let Some(status) = portable.status.as_ref() {
                status.is_portable.hash(hasher);
                format!("{:?}", status.status).hash(hasher);
                status.is_unlocked.hash(hasher);
                status.auto_unlock_enabled.hash(hasher);
            }
        }
        _ => {}
    }
}
