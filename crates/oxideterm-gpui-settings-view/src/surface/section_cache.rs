use crate::{SettingsTab, TerminalSettingsPage, entity::SettingsWorkspaceEntity};
use oxideterm_settings::{BackgroundScope, PersistedSettings};
use oxideterm_settings_model::{SettingsSectionId, ThemeTarget, is_custom_theme_id};
use std::hash::{DefaultHasher, Hash};

/// The settings surface owns its row height inputs; the window supplies only
/// counts and flags from features that have independent runtime owners.
pub fn hash_settings_section_layout(
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
