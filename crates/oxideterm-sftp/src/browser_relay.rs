// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use std::sync::Arc;

use crate::{
    BrowserTransferEvent, DirectoryProgressAccumulator, ProgressStore, RemoteRelayDisposition,
    RemoteRelayProgressContext, SftpError, SftpSession, SftpTransferManager,
    StoredTransferProgress, TransferDirection, TransferProtocol, TransferType,
    directory_progress::next_browser_progress,
};

pub struct SftpRelayRequest {
    pub transfer_id: String,
    pub direction: TransferDirection,
    pub is_directory: bool,
    pub source_path: String,
    pub destination_path: String,
    pub disposition: RemoteRelayDisposition,
    pub context: Option<RemoteRelayProgressContext>,
    pub resume: Option<StoredTransferProgress>,
}

pub async fn discard_sftp_relay_progress(
    destination: &SftpSession,
    store: &dyn ProgressStore,
    progress: &StoredTransferProgress,
) -> Result<(), SftpError> {
    destination.discard_remote_relay_progress(progress).await?;
    store.delete(&progress.transfer_id).await
}

pub async fn run_sftp_relay(
    source: SftpSession,
    destination: SftpSession,
    manager: Arc<SftpTransferManager>,
    progress_store: Arc<dyn ProgressStore>,
    request: SftpRelayRequest,
    emit: impl Fn(BrowserTransferEvent) + Send + Sync + 'static,
) -> Result<(), String> {
    let SftpRelayRequest {
        transfer_id,
        direction,
        is_directory,
        source_path,
        destination_path,
        disposition,
        context,
        resume,
    } = request;
    emit(BrowserTransferEvent::ProtocolResolved(
        TransferProtocol::Sftp,
    ));
    let (progress_tx, mut progress_rx) = tokio::sync::mpsc::channel(100);
    let progress_task = tokio::spawn(async move {
        let mut accumulator = is_directory.then(DirectoryProgressAccumulator::default);
        while let Some(progress) = next_browser_progress(&mut progress_rx, &mut accumulator).await {
            emit(BrowserTransferEvent::Progress {
                transferred: progress.transferred_bytes,
                total: progress.total_bytes,
                speed: progress.speed,
            });
        }
    });
    let result = if is_directory && resume.is_some() {
        drop(progress_tx);
        Err(SftpError::TransferError(
            "Remote directory relay restart resume is not available".to_owned(),
        ))
    } else if is_directory {
        source
            .relay_dir_to(
                &destination,
                &source_path,
                &destination_path,
                disposition,
                &transfer_id,
                Some(progress_tx),
                Some(manager),
            )
            .await
            .map(|_| ())
    } else if let Some(context) = context {
        let transfer_type = match direction {
            TransferDirection::Upload => TransferType::Upload,
            TransferDirection::Download => TransferType::Download,
        };
        source
            .relay_file_to(
                &destination,
                &source_path,
                &destination_path,
                disposition,
                &transfer_id,
                Some(progress_tx),
                Some(manager),
                progress_store,
                context,
                resume,
                transfer_type,
            )
            .await
            .map(|_| ())
    } else {
        drop(progress_tx);
        let _ = progress_task.await;
        return Err("Saved SFTP endpoint configuration is unavailable".to_owned());
    };
    let _ = progress_task.await;
    result.map_err(|error| error.to_string())
}
