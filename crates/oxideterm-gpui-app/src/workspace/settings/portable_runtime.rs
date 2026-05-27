use zeroize::Zeroizing;

const PORTABLE_SETTINGS_DIALOG_WIDTH: f32 = 460.0;
const PORTABLE_SETTINGS_PATH_CARD_GAP: f32 = 12.0;
const PORTABLE_SETTINGS_BUTTON_GAP: f32 = 12.0;

fn portable_activation_label(
    i18n: &oxideterm_i18n::I18n,
    activation: oxideterm_portable_runtime::PortableActivationKind,
) -> String {
    match activation {
        oxideterm_portable_runtime::PortableActivationKind::Marker => {
            i18n.t("settings_view.general.portable_activation_marker")
        }
        oxideterm_portable_runtime::PortableActivationKind::Config => {
            i18n.t("settings_view.general.portable_activation_config")
        }
        oxideterm_portable_runtime::PortableActivationKind::Disabled => {
            i18n.t("settings_view.general.portable_activation_disabled")
        }
    }
}

fn portable_status_badge_color(
    status: oxideterm_portable_runtime::PortableBootstrapStatus,
    tokens: &oxideterm_theme::ThemeTokens,
) -> u32 {
    match status {
        oxideterm_portable_runtime::PortableBootstrapStatus::Unlocked => tokens.ui.success,
        oxideterm_portable_runtime::PortableBootstrapStatus::Disabled => tokens.ui.text_muted,
        oxideterm_portable_runtime::PortableBootstrapStatus::NeedsSetup
        | oxideterm_portable_runtime::PortableBootstrapStatus::Locked => tokens.ui.warning,
    }
}

include!("portable_runtime/render.rs");
include!("portable_runtime/actions.rs");
include!("portable_runtime/dialogs.rs");
