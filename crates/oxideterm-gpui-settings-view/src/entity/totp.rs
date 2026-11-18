use super::*;
use oxideterm_connections::totp::{DEFAULT_TOTP_PROMPT, TotpAlgorithm, TotpCredential};

pub struct TotpDraft {
    pub id: Option<String>,
    pub name: String,
    pub secret: Zeroizing<String>,
    pub pattern: String,
    pub period: String,
    pub algorithm: TotpAlgorithm,
    pub digits: u8,
    pub enabled: bool,
    pub connection_ids: HashSet<String>,
}

impl TotpDraft {
    pub fn new(credential: Option<&TotpCredential>) -> Self {
        Self {
            id: credential.map(|entry| entry.id.clone()),
            name: credential
                .map(|entry| entry.name.clone())
                .unwrap_or_default(),
            secret: Zeroizing::new(String::new()),
            pattern: credential
                .map(|entry| entry.prompt_pattern.clone())
                .unwrap_or_else(|| DEFAULT_TOTP_PROMPT.into()),
            period: credential
                .map(|entry| entry.parameters.period)
                .unwrap_or(30)
                .to_string(),
            algorithm: credential
                .map(|entry| entry.parameters.algorithm)
                .unwrap_or_default(),
            digits: credential.map(|entry| entry.parameters.digits).unwrap_or(6),
            enabled: credential.is_none_or(|entry| entry.enabled),
            connection_ids: HashSet::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{SettingsInput, SettingsTab, entity::SettingsWorkspaceEntity};
    use gpui::AppContext;

    #[gpui::test]
    fn totp_editor_accepts_shared_input_and_clears_seed_on_page_change(
        cx: &mut gpui::TestAppContext,
    ) {
        let entity = cx.new(SettingsWorkspaceEntity::new);
        entity.update(cx, |settings, cx| {
            settings.set_active_tab(SettingsTab::Privilege, cx);
            settings.totp_draft = Some(TotpDraft::new(None));
            assert!(settings.focus_settings_entity_input(SettingsInput::TotpName, cx));
            assert!(settings.replace_settings_entity_input(
                SettingsInput::TotpName,
                None,
                "跳板验证码",
                cx
            ));
            assert_eq!(
                settings.settings_entity_input_value(SettingsInput::TotpName),
                Some("跳板验证码")
            );
            assert!(settings.focus_settings_entity_input(SettingsInput::TotpSecret, cx));
            assert!(settings.replace_settings_entity_input(
                SettingsInput::TotpSecret,
                None,
                "JBSWY3DPEHPK3PXP",
                cx
            ));
            assert_eq!(
                settings.settings_entity_input_value(SettingsInput::TotpSecret),
                Some("JBSWY3DPEHPK3PXP")
            );
            assert!(SettingsInput::TotpSecret.is_secret());
            settings.set_active_tab(SettingsTab::Terminal, cx);
            assert!(
                settings
                    .settings_entity_input_value(SettingsInput::TotpSecret)
                    .is_none()
            );
        });
    }
}
