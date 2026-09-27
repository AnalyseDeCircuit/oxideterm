// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

//! Local audit persistence, independent from notification and window lifetimes.

mod export;
mod instance;
mod key;
mod model;
mod operation;
mod recording;
mod redact;
mod scope;
mod service;
mod store;

pub use export::AuditExportFormat;
pub use key::{AuditKeyProvider, PlatformAuditKeyProvider};
pub use model::*;
pub use operation::{AuditContext, AuditOperation, AuditRegistration};
pub use recording::*;
pub use redact::redact;
pub use service::{AuditClient, AuditHealth, AuditService, RecordingSink};
pub use store::{AuditStore, DurableRecordingFiles, RecordingFiles};
