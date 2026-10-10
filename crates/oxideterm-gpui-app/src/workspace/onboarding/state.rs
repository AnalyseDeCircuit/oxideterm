use super::*;

pub(in crate::workspace) const ONBOARDING_TOTAL_STEPS: usize = 5;
pub(in crate::workspace) const ONBOARDING_STEPS_MOTION_ID: &str = "onboarding-steps";
pub(in crate::workspace) const ONBOARDING_ANIMATION_MOTION_ID: &str = "onboarding-animation";
pub(in crate::workspace) const ONBOARDING_ICON_SIZE: f32 = 16.0;
pub(in crate::workspace) const ONBOARDING_STEP_ICON_SLOT: f32 = 28.0;
pub(in crate::workspace) const ONBOARDING_ACCENT_SUBTLE_ALPHA: u32 = 0x0d; // Tauri accent/5.
pub(in crate::workspace) const ONBOARDING_ACCENT_BORDER_ALPHA: u32 = 0x33; // Tauri accent/20.
pub(in crate::workspace) const ONBOARDING_CARD_ALPHA: u32 = 0xcc; // Browser panels sit over the dialog backdrop but stay readable.
pub(in crate::workspace) const ONBOARDING_DISABLED_OPACITY: f32 = 0.45;

pub(in crate::workspace) const ONBOARDING_FONT_OPTIONS: [(FontFamily, &str, bool); 4] = [
    (FontFamily::Jetbrains, "JetBrains Mono NF (Subset)", true),
    (FontFamily::Meslo, "MesloLGM NF (Subset)", true),
    (FontFamily::Maple, "Maple Mono NF CN (Subset)", true),
    (FontFamily::Custom, "Custom...", false),
];

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::workspace) enum OnboardingStep {
    Welcome,
    Appearance,
    Workflow,
    Tools,
    QuickStart,
}

impl OnboardingStep {
    pub(in crate::workspace) fn from_index(index: usize) -> Self {
        match index {
            0 => Self::Welcome,
            1 => Self::Appearance,
            2 => Self::Workflow,
            3 => Self::Tools,
            _ => Self::QuickStart,
        }
    }

    pub(in crate::workspace) fn title_key(self) -> &'static str {
        match self {
            Self::Welcome => "onboarding.step_welcome",
            Self::Appearance => "onboarding.step_appearance",
            Self::Workflow => "onboarding.step_workflow",
            Self::Tools => "onboarding.step_tools",
            Self::QuickStart => "onboarding.step_start",
        }
    }
}

#[derive(Clone)]
pub(in crate::workspace) struct OnboardingState {
    pub(in crate::workspace) open: bool,
    pub(in crate::workspace) save_failed: bool,
    pub(in crate::workspace) step: usize,
    pub(in crate::workspace) disclaimer_accepted: bool,
    pub(in crate::workspace) ai_opt_in: bool,
    pub(in crate::workspace) tool_use_opt_in: bool,
    pub(in crate::workspace) import_state: OnboardingImportState,
    pub(in crate::workspace) imported_count: usize,
    pub(in crate::workspace) host_count: Option<usize>,
    pub(in crate::workspace) scroll_handle: ScrollHandle,
}

impl OnboardingState {
    pub(in crate::workspace) fn from_settings(settings: &PersistedSettings) -> Self {
        Self {
            open: !settings.onboarding_completed,
            save_failed: false,
            step: 0,
            disclaimer_accepted: disclaimer_accepted_from_settings(settings),
            ai_opt_in: settings.ai.enabled,
            tool_use_opt_in: settings.ai.enabled && settings.ai.tool_use.enabled,
            import_state: OnboardingImportState::Idle,
            imported_count: 0,
            host_count: None,
            scroll_handle: ScrollHandle::new(),
        }
    }

    pub(in crate::workspace) fn reset_for_open(&mut self, settings: &PersistedSettings) {
        self.open = true;
        self.save_failed = false;
        self.step = 0;
        self.disclaimer_accepted = disclaimer_accepted_from_settings(settings);
        self.ai_opt_in = settings.ai.enabled;
        self.tool_use_opt_in = settings.ai.enabled && settings.ai.tool_use.enabled;
        self.import_state = OnboardingImportState::Idle;
        self.imported_count = 0;
        self.host_count = None;
        self.scroll_handle = ScrollHandle::new();
    }
}

fn disclaimer_accepted_from_settings(settings: &PersistedSettings) -> bool {
    // Completed legacy onboarding flows necessarily passed the disclaimer step.
    settings.onboarding_disclaimer_accepted || settings.onboarding_completed
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::workspace) enum OnboardingImportState {
    Idle,
    Loading,
    Done,
}

#[cfg(test)]
mod tests {
    use super::disclaimer_accepted_from_settings;
    use oxideterm_settings::PersistedSettings;

    #[test]
    fn persisted_and_legacy_onboarding_accept_the_disclaimer() {
        for mut settings in [
            PersistedSettings {
                onboarding_disclaimer_accepted: true,
                ..PersistedSettings::default()
            },
            PersistedSettings {
                onboarding_completed: true,
                ..PersistedSettings::default()
            },
        ] {
            assert!(disclaimer_accepted_from_settings(&settings));
            settings.onboarding_disclaimer_accepted = false;
            settings.onboarding_completed = false;
            assert!(!disclaimer_accepted_from_settings(&settings));
        }
    }
}
