// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

//! Private, bounded signing transport between SSH authentication and a FIDO provider.

mod protocol;
pub use protocol::*;

mod client;
pub use client::{SecurityKeyInteraction, SecurityKeyProvider};
