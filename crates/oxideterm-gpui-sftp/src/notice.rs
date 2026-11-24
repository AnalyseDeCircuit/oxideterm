// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TerminalNoticeVariant {
    Default,
    Success,
    Error,
    Warning,
}

#[derive(Clone, Debug)]
pub struct TerminalNotice {
    pub title: String,
    pub description: Option<String>,
    pub status_text: Option<String>,
    pub progress: Option<f32>,
    pub variant: TerminalNoticeVariant,
}
