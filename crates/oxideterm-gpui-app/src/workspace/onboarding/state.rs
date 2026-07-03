use super::*;

pub(in crate::workspace) const ONBOARDING_TOTAL_STEPS: usize = 9;
pub(in crate::workspace) const ONBOARDING_WIDTH: f32 = 800.0; // Tauri DialogContent sm:max-w-[800px].
pub(in crate::workspace) const ONBOARDING_MAX_HEIGHT: f32 = 720.0;
pub(in crate::workspace) const ONBOARDING_PROGRESS_ICON_SIZE: f32 = 28.0; // Tauri progress buttons w-7 h-7.
pub(in crate::workspace) const ONBOARDING_ICON_SIZE: f32 = 16.0;
pub(in crate::workspace) const ONBOARDING_STEP_ICON_SLOT: f32 = 28.0;
pub(in crate::workspace) const ONBOARDING_THEME_CARD_HEIGHT: f32 = 104.0;
pub(in crate::workspace) const ONBOARDING_THEME_PREVIEW_HEIGHT: f32 = 72.0;
pub(in crate::workspace) const ONBOARDING_ACCENT_SUBTLE_ALPHA: u32 = 0x0d; // Tauri accent/5.
pub(in crate::workspace) const ONBOARDING_ACCENT_BORDER_ALPHA: u32 = 0x33; // Tauri accent/20.
pub(in crate::workspace) const ONBOARDING_ACCENT_STRONG_BORDER_ALPHA: u32 = 0x66; // Tauri accent/40.
pub(in crate::workspace) const ONBOARDING_CARD_ALPHA: u32 = 0xcc; // Browser panels sit over the dialog backdrop but stay readable.
pub(in crate::workspace) const ONBOARDING_DISABLED_OPACITY: f32 = 0.45;

pub(in crate::workspace) const ONBOARDING_THEME_IDS: [&str; 8] = [
    "default",
    "oxide",
    "dracula",
    "nord",
    "catppuccin-mocha",
    "tokyo-night",
    "paper-oxide",
    "rose-pine",
];

pub(in crate::workspace) const ONBOARDING_FONT_OPTIONS: [(FontFamily, &str, bool); 6] = [
    (FontFamily::Jetbrains, "JetBrains Mono NF (Subset)", true),
    (FontFamily::Meslo, "MesloLGM NF (Subset)", true),
    (FontFamily::Maple, "Maple Mono NF CN (Subset)", true),
    (FontFamily::Cascadia, "Cascadia Code", false),
    (FontFamily::Consolas, "Consolas", false),
    (FontFamily::Menlo, "Menlo", false),
];

pub(in crate::workspace) const ONBOARDING_LANGUAGES: [(Language, &str); 11] = [
    (Language::En, "English"),
    (Language::ZhCn, "简体中文"),
    (Language::ZhTw, "繁體中文"),
    (Language::Ja, "日本語"),
    (Language::Ko, "한국어"),
    (Language::FrFr, "Français"),
    (Language::De, "Deutsch"),
    (Language::EsEs, "Español"),
    (Language::It, "Italiano"),
    (Language::PtBr, "Português (BR)"),
    (Language::Vi, "Tiếng Việt"),
];

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::workspace) enum OnboardingStep {
    Welcome,
    Disclaimer,
    Appearance,
    Workflow,
    Features,
    AiIntro,
    AiSetup,
    CliCompanion,
    QuickStart,
}

impl OnboardingStep {
    pub(in crate::workspace) fn from_index(index: usize) -> Self {
        match index {
            0 => Self::Welcome,
            1 => Self::Disclaimer,
            2 => Self::Appearance,
            3 => Self::Workflow,
            4 => Self::Features,
            5 => Self::AiIntro,
            6 => Self::AiSetup,
            7 => Self::CliCompanion,
            _ => Self::QuickStart,
        }
    }

    pub(in crate::workspace) fn icon(self) -> LucideIcon {
        match self {
            Self::Welcome => LucideIcon::Home,
            Self::Disclaimer => LucideIcon::FileText,
            Self::Appearance => LucideIcon::Monitor,
            Self::Workflow => LucideIcon::Network,
            Self::Features => LucideIcon::Shield,
            Self::AiIntro => LucideIcon::Sparkles,
            Self::AiSetup => LucideIcon::Settings,
            Self::CliCompanion => LucideIcon::Terminal,
            Self::QuickStart => LucideIcon::Rocket,
        }
    }
}

#[derive(Clone)]
pub(in crate::workspace) struct OnboardingState {
    pub(in crate::workspace) open: bool,
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
        let mut state = Self {
            open: !settings.onboarding_completed,
            step: 0,
            disclaimer_accepted: false,
            ai_opt_in: settings.ai.enabled,
            tool_use_opt_in: settings.ai.enabled && settings.ai.tool_use.enabled,
            import_state: OnboardingImportState::Idle,
            imported_count: 0,
            host_count: None,
            scroll_handle: ScrollHandle::new(),
        };
        if settings.onboarding_completed {
            state.disclaimer_accepted = true;
        }
        state
    }

    pub(in crate::workspace) fn reset_for_open(&mut self, settings: &PersistedSettings) {
        self.open = true;
        self.step = 0;
        self.disclaimer_accepted = false;
        self.ai_opt_in = settings.ai.enabled;
        self.tool_use_opt_in = settings.ai.enabled && settings.ai.tool_use.enabled;
        self.import_state = OnboardingImportState::Idle;
        self.imported_count = 0;
        self.host_count = None;
        self.scroll_handle = ScrollHandle::new();
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::workspace) enum OnboardingImportState {
    Idle,
    Loading,
    Done,
}
