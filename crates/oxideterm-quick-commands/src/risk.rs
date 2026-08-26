// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

//! Risk classification for reusable terminal commands.

use regex::Regex;
use std::sync::OnceLock;

/// Describes the confirmation risk associated with a quick command.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum QuickCommandRisk {
    Medium,
    High,
}

/// Classifies commands using the same boundaries as the Tauri implementation.
pub fn classify_command_risk(command: &str) -> Option<QuickCommandRisk> {
    if command_matches_patterns(command, high_risk_command_patterns()) {
        return Some(QuickCommandRisk::High);
    }
    if command_matches_patterns(command, medium_risk_command_patterns()) {
        return Some(QuickCommandRisk::Medium);
    }
    None
}

fn command_matches_patterns(command: &str, patterns: &[Regex]) -> bool {
    patterns.iter().any(|pattern| pattern.is_match(command))
}

fn high_risk_command_patterns() -> &'static [Regex] {
    // Keep these patterns in semantic lockstep with Tauri's
    // lib/terminal/completion/risk.ts classifier.
    static PATTERNS: OnceLock<Vec<Regex>> = OnceLock::new();
    PATTERNS.get_or_init(|| {
        [
            r"(?i)\brm\s+-(?:[^\s]*r[^\s]*f|[^\s]*f[^\s]*r)\b",
            r"(?i)\bkubectl\s+delete\b",
            r"(?i)\bsystemctl\s+(?:stop|restart|disable|kill)\b",
            r"(?i)\bdocker\s+(?:rm|rmi|system\s+prune|container\s+prune|volume\s+prune|network\s+prune)\b",
            r"(?i)\b(?:shutdown|reboot|halt|poweroff)\b",
            r"(?i)\bkill(?:all)?\s+-9\b",
            r"(?i)\bmkfs(?:\.[^\s]+)?\b",
            r"(?i)\bdd\s+.*\bof=",
            r"(?i)\bchmod\s+-R\b",
            r"(?i)\bchown\s+-R\b",
        ]
        .into_iter()
        .map(|pattern| Regex::new(pattern).expect("quick command risk pattern must compile"))
        .collect()
    })
}

fn medium_risk_command_patterns() -> &'static [Regex] {
    // Keep these patterns in semantic lockstep with Tauri's
    // lib/terminal/completion/risk.ts classifier.
    static PATTERNS: OnceLock<Vec<Regex>> = OnceLock::new();
    PATTERNS.get_or_init(|| {
        [r"(?i)\bsudo\b", r"(?i)\bchmod\s+(?:-R\s+)?777\b"]
            .into_iter()
            .map(|pattern| Regex::new(pattern).expect("quick command risk pattern must compile"))
            .collect()
    })
}
