// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};

// Settings structs are split by settings page/semantic area, but remain
// included from this module so serde field names and public exports do not move.
include!("model/base.rs");
include!("model/highlight.rs");
include!("model/terminal.rs");
include!("model/ui_connection.rs");
include!("model/ai.rs");
include!("model/misc.rs");
