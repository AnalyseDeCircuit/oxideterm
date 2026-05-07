// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use crate::PreviewSessionState;

pub trait PreviewRenderer {
    type Output;

    fn render_preview(&self, state: &PreviewSessionState) -> Self::Output;
}
