// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CloudSyncProgress {
    pub stage: CloudSyncProgressStage,
    pub current: usize,
    pub total: usize,
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
        current,
        total,
        message: None,
    });
}
