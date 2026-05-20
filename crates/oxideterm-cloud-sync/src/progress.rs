// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

#[derive(Clone, Debug, PartialEq)]
pub struct CloudSyncProgress {
    pub stage: CloudSyncProgressStage,
    pub current: f64,
    pub total: f64,
    pub message: Option<String>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum CloudSyncProgressStage {
    FetchMetadata,
    Preflight,
    Exporting,
    UploadingBlob,
    Downloading,
    Validating,
    PreviewingImport,
    Importing,
    CreatingBackup,
    Done,
}

pub trait CloudSyncProgressSink: Send {
    fn report(&mut self, progress: CloudSyncProgress);
}

impl<F> CloudSyncProgressSink for F
where
    F: FnMut(CloudSyncProgress) + Send,
{
    fn report(&mut self, progress: CloudSyncProgress) {
        self(progress);
    }
}

pub fn report_progress(
    sink: &mut dyn CloudSyncProgressSink,
    stage: CloudSyncProgressStage,
    current: usize,
    total: usize,
) {
    sink.report(CloudSyncProgress {
        stage,
        current: current as f64,
        total: total as f64,
        message: None,
    });
}

pub fn report_fractional_progress(
    sink: &mut dyn CloudSyncProgressSink,
    stage: CloudSyncProgressStage,
    current: f64,
    total: f64,
) {
    sink.report(CloudSyncProgress {
        stage,
        current,
        total,
        message: None,
    });
}
