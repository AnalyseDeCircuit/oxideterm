// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use oxideterm_settings::UpdateChannel;

pub const STABLE_UPDATE_ENDPOINT: &str =
    "https://github.com/AnalyseDeCircuit/oxideterm/releases/latest/download/latest.json";
pub const BETA_UPDATE_ENDPOINT: &str =
    "https://github.com/AnalyseDeCircuit/oxideterm/releases/download/updater-beta/latest.json";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct UpdateEndpoint {
    pub channel: UpdateChannel,
    pub url: &'static str,
}

pub fn endpoint_for_channel(channel: UpdateChannel) -> UpdateEndpoint {
    let url = match channel {
        UpdateChannel::Stable => STABLE_UPDATE_ENDPOINT,
        UpdateChannel::Beta => BETA_UPDATE_ENDPOINT,
    };
    UpdateEndpoint { channel, url }
}
