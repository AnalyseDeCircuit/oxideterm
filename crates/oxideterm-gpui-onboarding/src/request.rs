// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use oxideterm_settings::{AnimationSpeed, AppearanceScheme, CornerStyle, FontFamily};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OnboardingThemeTarget {
    Application,
    Terminal,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OnboardingRequest {
    ToggleDisclaimer,
    OpenLegalNotice,
    GoToStep(usize),
    Next,
    Back,
    SkipToLaunch,
    Complete,
    CloseOpenSelect,
    SetAppearanceMode(Option<AppearanceScheme>),
    SetEditScheme(bool),
    AssignCurrentScheme(bool),
    SetFontFamily(FontFamily),
    AdjustFontSize(i64),
    SetCornerStyle(CornerStyle),
    SetAnimationSpeed(AnimationSpeed),
    ToggleAiOptIn,
    ToggleToolUseOptIn,
    ImportSshHosts,
    RefreshCliStatus,
    OpenTerminal,
    OpenNewConnection,
    OpenConnectionImporters,
    OpenCliSettings,
}
