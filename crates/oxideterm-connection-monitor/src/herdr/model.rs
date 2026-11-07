// Copyright (C) 2026 jaime.su
// SPDX-License-Identifier: GPL-3.0-only

//! Snapshot shapes rendered by the monitor UI and the joins it runs over
//! them.
//!
//! These are the parser's output, not a wire format: `herdr/parse.rs` reads
//! herdr's snake_case JSON by hand, so nothing here is deserialized and no
//! field claims a name the envelopes do not use. The panel renders the
//! provider-neutral shapes in `crate::multiplexer`, never these.

use std::collections::{HashMap, HashSet};

/// How a probe capture answered, as classified by `herdr/parse.rs`. Every
/// capture lands in exactly one state, and the callers branch on it instead
/// of inferring health from empty lists.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum HerdrStatus {
    /// The probe printed nothing, so its answer carries no verdict at all.
    /// Only the exit code of the invocation (session list) or the panel's own
    /// gate (subtree) can say whether herdr ran, and neither is the capture.
    Unknown,
    /// The herdr server answered, whether or not it had rows to report.
    Available,
    /// The `herdr` binary is not installed.
    Unavailable,
    /// The herdr CLI exists but its server is not running; launching `herdr`
    /// in a terminal starts it.
    ServerNotRunning,
    /// The answer was an error envelope, or a capture whose structure could
    /// not be trusted (a list member of the wrong type, or text the probe
    /// could not have printed). `message` is the server's own text in the
    /// first case and a bounded diagnostic naming the broken shape in the
    /// second; either way it is sanitized and length-limited.
    Error { message: String },
    /// The capture contradicted the probe's own framing: a list member of the
    /// wrong type, a document the probe cannot print, or text with no framing
    /// at all. Nothing here is the server's own words, so the panel supplies
    /// its localized failure text instead of showing a sentence from the app.
    Malformed,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HerdrSession {
    pub name: String,
    /// A stopped session still accepts `session attach`, which starts it.
    pub running: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HerdrWorkspace {
    pub workspace_id: String,
    pub label: String,
    pub number: u32,
    pub focused: bool,
    pub pane_count: u32,
    pub tab_count: u32,
    pub agent_status: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HerdrTab {
    pub tab_id: String,
    pub workspace_id: String,
    /// The tab name shown in herdr's tab strip.
    pub label: Option<String>,
    pub number: u32,
    pub pane_count: u32,
    pub focused: bool,
    pub agent_status: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HerdrPane {
    pub pane_id: String,
    pub workspace_id: String,
    pub tab_id: Option<String>,
    /// The pane's own name set through `pane rename`; herdr shows it in the
    /// tab strip, so it is the name users recognize.
    pub label: Option<String>,
    /// Latest OSC 0/2 terminal title. Dynamic and independent of `label`.
    pub terminal_title: Option<String>,
    pub cwd: Option<String>,
    pub focused: bool,
    pub agent_status: Option<String>,
    /// The remote keeps its restore diagnostics server-side; only whether the
    /// pane failed to restore is worth a row badge.
    pub restore_failed: bool,
}

/// Agent rows are joined onto panes by id; every field except the pane
/// reference is optional because agent detection varies by herdr release and
/// agent kind.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HerdrAgent {
    pub pane_id: String,
    pub workspace_id: Option<String>,
    pub label: Option<String>,
    pub status: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HerdrSnapshot {
    pub status: HerdrStatus,
    pub sessions: Vec<HerdrSession>,
}

/// Everything beneath one session, plus the classification of the capture it
/// came from. Fetched per session because every herdr workspace command is
/// scoped to a session socket.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HerdrSessionDetail {
    pub status: HerdrStatus,
    pub workspaces: Vec<HerdrWorkspace>,
    pub tabs: Vec<HerdrTab>,
    pub panes: Vec<HerdrPane>,
    pub agents: Vec<HerdrAgent>,
}

/// The agent list joined onto panes, built in one pass. Tree construction
/// reads one agent per pane row, so scanning `agents` per row would make it
/// quadratic in the rows the server already sent flat.
pub struct HerdrAgentJoin<'a> {
    by_pane: HashMap<&'a str, &'a HerdrAgent>,
    orphans: Vec<&'a HerdrAgent>,
}

impl<'a> HerdrAgentJoin<'a> {
    /// The agent detected in a pane. When the server repeats a pane id the
    /// first row wins, because that is the row herdr listed first.
    pub fn for_pane(&self, pane_id: &str) -> Option<&'a HerdrAgent> {
        self.by_pane.get(pane_id).copied()
    }

    /// Agents that reference a pane the pane list does not carry; they render
    /// in a fallback group instead of disappearing.
    pub fn orphans(&self) -> &[&'a HerdrAgent] {
        &self.orphans
    }
}

impl HerdrSessionDetail {
    pub fn agent_join(&self) -> HerdrAgentJoin<'_> {
        let known_panes: HashSet<&str> = self
            .panes
            .iter()
            .map(|pane| pane.pane_id.as_str())
            .collect();
        let mut by_pane = HashMap::with_capacity(self.agents.len());
        let mut orphans = Vec::new();
        for agent in &self.agents {
            let pane_id = agent.pane_id.as_str();
            by_pane.entry(pane_id).or_insert(agent);
            if !known_panes.contains(pane_id) {
                orphans.push(agent);
            }
        }
        HerdrAgentJoin { by_pane, orphans }
    }
}
