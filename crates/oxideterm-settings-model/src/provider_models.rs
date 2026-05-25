// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

//! Compact provider model chip view models.

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AiProviderModelChipItem {
    pub model: String,
    pub active: bool,
}

pub fn ai_provider_model_chip_rows(
    provider: &oxideterm_ai::AiProviderView,
    visible_model_count: usize,
    chips_per_row: usize,
) -> Vec<Vec<AiProviderModelChipItem>> {
    if chips_per_row == 0 {
        return Vec::new();
    }
    provider
        .models
        .iter()
        .take(visible_model_count)
        .map(|model| AiProviderModelChipItem {
            model: model.clone(),
            active: provider.default_model == *model,
        })
        .collect::<Vec<_>>()
        .chunks(chips_per_row)
        .map(|row| row.to_vec())
        .collect()
}
