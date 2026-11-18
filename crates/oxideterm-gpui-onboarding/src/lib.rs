// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

//! First-run onboarding view and step renderers.
//!
//! This crate owns onboarding layout, step-transition motion, and localized UI
//! refreshes without depending on `oxideterm-gpui-app`.

mod request;
mod steps;
mod types;
mod view;
mod widgets;

pub use request::{OnboardingRequest, OnboardingThemeTarget};
pub use types::{
    ONBOARDING_FONT_OPTIONS, ONBOARDING_TOTAL_STEPS, OnboardingCliSnapshot, OnboardingFrame,
    OnboardingImportStatus, OnboardingShortcutItem, OnboardingStep, OnboardingViewBindings,
};
pub use view::OnboardingView;
