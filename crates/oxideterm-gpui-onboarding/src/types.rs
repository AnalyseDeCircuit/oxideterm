// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use std::rc::Rc;

use gpui::{AnyElement, App, ImageSource, SharedString, Window};
use oxideterm_i18n::I18n;
use oxideterm_settings::{AnimationSpeed, FontFamily, PersistedSettings};
use oxideterm_theme::ThemeTokens;

use crate::{OnboardingRequest, OnboardingThemeTarget};

pub const ONBOARDING_TOTAL_STEPS: usize = 4;

pub const ONBOARDING_FONT_OPTIONS: [(FontFamily, &str, bool); 4] = [
    (FontFamily::Jetbrains, "JetBrains Mono NF (Subset)", true),
    (FontFamily::Meslo, "MesloLGM NF (Subset)", true),
    (FontFamily::Maple, "Maple Mono NF CN (Subset)", true),
    (FontFamily::Custom, "Custom...", false),
];

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OnboardingStep {
    Overview,
    Disclaimer,
    Appearance,
    Launch,
}

impl OnboardingStep {
    pub const DISCLAIMER_INDEX: usize = 1;

    pub fn from_index(index: usize) -> Self {
        match index {
            0 => Self::Overview,
            1 => Self::Disclaimer,
            2 => Self::Appearance,
            _ => Self::Launch,
        }
    }

    pub fn title_key(self) -> &'static str {
        match self {
            Self::Overview => "onboarding.step_overview",
            Self::Disclaimer => "onboarding.step_disclaimer",
            Self::Appearance => "onboarding.step_appearance",
            Self::Launch => "onboarding.step_start",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OnboardingImportStatus {
    Idle,
    Loading,
    Done,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct OnboardingCliSnapshot {
    pub loading: bool,
    pub installed: bool,
    pub needs_reinstall: bool,
    pub bundled: bool,
    pub has_status: bool,
    pub install_path: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OnboardingShortcutItem {
    pub label_key: &'static str,
    pub key_label: String,
}

/// Read-only render snapshot supplied by the host workspace.
pub struct OnboardingFrame {
    pub tokens: ThemeTokens,
    pub preview_tokens: ThemeTokens,
    pub preview_theme_name: String,
    pub i18n: I18n,
    pub settings: PersistedSettings,
    pub sidebar_occupied_width: f32,
    pub has_background: bool,
    pub system_dark: bool,
    pub edit_scheme: Option<bool>,
    pub app_icon_source: ImageSource,
    pub ui_font_family: SharedString,
    pub mono_font_family: SharedString,
    pub language_label: String,
    pub animation_options: Vec<(AnimationSpeed, String)>,
    pub open_settings_select_active: bool,
    pub step: usize,
    pub disclaimer_accepted: bool,
    pub save_failed: bool,
    pub ai_opt_in: bool,
    pub tool_use_opt_in: bool,
    pub import_status: OnboardingImportStatus,
    pub imported_count: usize,
    pub host_count: Option<usize>,
    pub cli: OnboardingCliSnapshot,
    pub shortcuts: Vec<OnboardingShortcutItem>,
}

pub struct OnboardingViewBindings {
    pub frame: Rc<dyn Fn(&App) -> Option<OnboardingFrame>>,
    pub dispatch: Rc<dyn Fn(OnboardingRequest, &mut Window, &mut App)>,
    pub language_select: Rc<dyn Fn(&ThemeTokens, &str, &mut App) -> AnyElement>,
    pub theme_select: Rc<dyn Fn(OnboardingThemeTarget, &PersistedSettings, &mut App) -> AnyElement>,
    pub custom_font_input: Rc<dyn Fn(&PersistedSettings, &mut App) -> AnyElement>,
    pub radius_slider: Rc<dyn Fn(i64, &mut App) -> AnyElement>,
}
