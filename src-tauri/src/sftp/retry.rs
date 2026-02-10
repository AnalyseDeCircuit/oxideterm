//! SFTP Transfer Retry Logic
//!
//! Provides automatic retry with exponential backoff for failed transfers.

use crate::sftp::error::SftpError;
use crate::sftp::progress::{ProgressStore, StoredTransferProgress};
use std::time::Duration;
use tracing::{info, warn};

/// Retry configuration
#[derive(Debug, Clone)]
pub struct RetryConfig {
    /// Maximum number of retry attempts
    pub max_retries: usize,

    /// Initial backoff duration in seconds
    pub initial_backoff_secs: u64,

    /// Backoff multiplier for each retry
    pub backoff_multiplier: f64,

    /// Maximum backoff duration in seconds
    pub max_backoff_secs: u64,
}

impl Default for RetryConfig {
    fn default() -> Self {
        Self {
            max_retries: 3,
            initial_backoff_secs: 1,
            backoff_multiplier: 2.0,
            max_backoff_secs: 30,
        }
    }
}

impl RetryConfig {
    /// Create a new retry configuration
    pub fn new(max_retries: usize) -> Self {
        Self {
            max_retries,
            ..Default::default()
        }
    }

    /// Set custom backoff parameters
    pub fn with_backoff(mut self, initial_secs: u64, multiplier: f64, max_secs: u64) -> Self {
        self.initial_backoff_secs = initial_secs;
        self.backoff_multiplier = multiplier;
        self.max_backoff_secs = max_secs;
        self
    }
}

/// Calculate backoff delay for a given retry attempt (exponential backoff)
pub fn calculate_backoff(attempt: usize, config: &RetryConfig) -> Duration {
    let delay_secs = (config.initial_backoff_secs as f64
        * config.backoff_multiplier.powi(attempt as i32))
    .min(config.max_backoff_secs as f64);

    Duration::from_secs(delay_secs as u64)
}

/// Check if an error is retryable
pub fn is_retryable_error(error: &SftpError) -> bool {
    match error {
        // Network errors are retryable
        SftpError::IoError(_) => true,
        SftpError::ChannelError(_) => true,
        SftpError::ProtocolError(msg) if msg.contains("timeout") => true,
        SftpError::ProtocolError(msg) if msg.contains("connection") => true,
        // Transfer errors are retryable
        SftpError::TransferError(_) => true,
        // Other errors are not retryable
        _ => false,
    }
}

/// Execute a transfer with automatic retry on failure
///
/// # Arguments
/// * `transfer_fn` - The transfer function to execute (returns bytes transferred)
/// * `config` - Retry configuration
/// * `progress_store` - Progress store for tracking state
/// * `progress` - Initial progress record
/// * `control` - Optional transfer control for pause/cancel
///
/// # Returns
/// * Total bytes transferred on success
/// * Error if all retries are exhausted or transfer is cancelled
pub async fn transfer_with_retry<F, Fut>(
    transfer_fn: F,
    config: RetryConfig,
    progress_store: std::sync::Arc<dyn ProgressStore>,
    mut progress: StoredTransferProgress,
    control: Option<std::sync::Arc<crate::sftp::TransferControl>>,
) -> Result<u64, SftpError>
where
    F: Fn() -> Fut + Send + Sync,
    Fut: std::future::Future<Output = Result<u64, SftpError>> + Send,
{
    let mut last_error_msg: Option<String> = None;

    for attempt in 0..=config.max_retries {
        // Check for cancellation before starting attempt
        if let Some(ref ctrl) = control {
            if ctrl.is_cancelled() {
                info!("Transfer {} cancelled by user", progress.transfer_id);
                progress.mark_cancelled();
                progress_store.save(&progress).await.map_err(|e| {
                    SftpError::StorageError(format!("Failed to save progress: {}", e))
                })?;
                return Err(SftpError::TransferCancelled);
            }
        }

        // Mark as active for this attempt
        progress.mark_active();
        progress_store
            .save(&progress)
            .await
            .map_err(|e| SftpError::StorageError(format!("Failed to save progress: {}", e)))?;

        info!(
            "Transfer attempt {}/{} for {}",
            attempt + 1,
            config.max_retries + 1,
            progress.transfer_id
        );

        match transfer_fn().await {
            Ok(transferred_bytes) => {
                // Success! Update progress to completed
                progress.mark_completed();
                progress_store.save(&progress).await.map_err(|e| {
                    SftpError::StorageError(format!("Failed to save progress: {}", e))
                })?;

                info!(
                    "Transfer {} completed successfully: {} bytes",
                    progress.transfer_id, transferred_bytes
                );

                return Ok(transferred_bytes);
            }
            Err(e) => {
                last_error_msg = Some(e.to_string());

                // Check if error is retryable
                if !is_retryable_error(&e) {
                    // Non-retryable error, fail immediately
                    progress.mark_failed(e.to_string());
                    progress_store.save(&progress).await.map_err(|e2| {
                        SftpError::StorageError(format!("Failed to save progress: {}", e2))
                    })?;

                    warn!(
                        "Transfer {} failed with non-retryable error: {}",
                        progress.transfer_id, e
                    );

                    return Err(e);
                }

                // Update progress to failed (but retryable)
                progress.mark_failed(e.to_string());
                progress_store.save(&progress).await.map_err(|e2| {
                    SftpError::StorageError(format!("Failed to save progress: {}", e2))
                })?;

                // If this is not the last attempt, wait and retry
                if attempt < config.max_retries {
                    let delay = calculate_backoff(attempt, &config);
                    info!(
                        "Transfer {} failed, retrying in {:?}: {}",
                        progress.transfer_id, delay, e
                    );

                    // Allow cancellation during backoff sleep
                    if let Some(ref ctrl) = control {
                        let mut cancel_rx = ctrl.subscribe_cancellation();
                        tokio::select! {
                            _ = tokio::time::sleep(delay) => {
                                // Sleep completed normally, continue to retry
                            }
                            _ = cancel_rx.changed() => {
                                if *cancel_rx.borrow() {
                                    // Cancelled during backoff
                                    info!("Transfer {} cancelled during backoff", progress.transfer_id);
                                    progress.mark_cancelled();
                                    progress_store
                                        .save(&progress)
                                        .await
                                        .map_err(|e| SftpError::StorageError(format!("Failed to save progress: {}", e)))?;
                                    return Err(SftpError::TransferCancelled);
                                }
                            }
                        }
                    } else {
                        tokio::time::sleep(delay).await;
                    }
                }
            }
        }
    }

    // All retries exhausted
    let error_msg = format!(
        "Transfer failed after {} attempts: {}",
        config.max_retries + 1,
        last_error_msg.unwrap_or_else(|| "Unknown error".to_string())
    );

    Err(SftpError::TransferError(error_msg))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    #[test]
    fn test_calculate_backoff() {
        let config = RetryConfig::default();

        // Attempt 0: 1 second
        let delay = calculate_backoff(0, &config);
        assert_eq!(delay.as_secs(), 1);

        // Attempt 1: 2 seconds
        let delay = calculate_backoff(1, &config);
        assert_eq!(delay.as_secs(), 2);

        // Attempt 2: 4 seconds
        let delay = calculate_backoff(2, &config);
        assert_eq!(delay.as_secs(), 4);

        // Attempt 5: should cap at max_backoff_secs (30)
        let delay = calculate_backoff(5, &config);
        assert_eq!(delay.as_secs(), 30);
    }

    #[test]
    fn test_calculate_backoff_custom_config() {
        let config = RetryConfig {
            max_retries: 5,
            initial_backoff_secs: 2,
            backoff_multiplier: 3.0,
            max_backoff_secs: 60,
        };

        // Attempt 0: 2 seconds
        let delay = calculate_backoff(0, &config);
        assert_eq!(delay.as_secs(), 2);

        // Attempt 1: 6 seconds (2 * 3)
        let delay = calculate_backoff(1, &config);
        assert_eq!(delay.as_secs(), 6);

        // Attempt 2: 18 seconds (6 * 3)
        let delay = calculate_backoff(2, &config);
        assert_eq!(delay.as_secs(), 18);
    }

    #[test]
    fn test_is_retryable_error() {
        // Network errors are retryable
        let io_err = SftpError::IoError(std::io::Error::new(
            std::io::ErrorKind::ConnectionReset,
            "Connection reset",
        ));
        assert!(is_retryable_error(&io_err));

        // Channel errors are retryable
        let channel_err = SftpError::ChannelError("Channel closed".to_string());
        assert!(is_retryable_error(&channel_err));

        // Timeout errors are retryable
        let timeout_err = SftpError::ProtocolError("Connection timeout".to_string());
        assert!(is_retryable_error(&timeout_err));

        // Permission errors are NOT retryable
        let perm_err = SftpError::PermissionDenied("Access denied".to_string());
        assert!(!is_retryable_error(&perm_err));

        // File not found errors are NOT retryable
        let not_found_err = SftpError::FileNotFound("/path/to/file".to_string());
        assert!(!is_retryable_error(&not_found_err));
    }

    #[tokio::test]
    async fn test_transfer_with_retry_success_on_first_attempt() {
        let call_count = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));

        let transfer_fn = {
            let call_count = call_count.clone();
            move || {
                let call_count = call_count.clone();
                async move {
                    call_count.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                    Ok(1024)
                }
            }
        };

        // Mock progress store (no-op for now)
        struct MockStore;
        #[async_trait::async_trait]
        impl ProgressStore for MockStore {
            async fn save(&self, _progress: &StoredTransferProgress) -> Result<(), SftpError> {
                Ok(())
            }
            async fn load(&self, _id: &str) -> Result<Option<StoredTransferProgress>, SftpError> {
                Ok(None)
            }
            async fn list_incomplete(
                &self,
                _id: &str,
            ) -> Result<Vec<StoredTransferProgress>, SftpError> {
                Ok(vec![])
            }
            async fn list_all_incomplete(&self) -> Result<Vec<StoredTransferProgress>, SftpError> {
                Ok(vec![])
            }
            async fn delete(&self, _id: &str) -> Result<(), SftpError> {
                Ok(())
            }
            async fn delete_for_session(&self, _id: &str) -> Result<(), SftpError> {
                Ok(())
            }
        }

        let progress = StoredTransferProgress::new(
            "test-1".to_string(),
            crate::sftp::progress::TransferType::Download,
            "/remote/file.txt".into(),
            "/local/file.txt".into(),
            2048,
            "session-1".to_string(),
        );

        let result = transfer_with_retry(
            transfer_fn,
            RetryConfig::default(),
            Arc::new(MockStore),
            progress,
            None, // No transfer control for basic test
        )
        .await;

        assert!(result.is_ok());
        assert_eq!(result.unwrap(), 1024);
        assert_eq!(call_count.load(std::sync::atomic::Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn test_transfer_with_retry_success_after_retries() {
        let attempt_count = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));

        let transfer_fn = {
            let attempt_count = attempt_count.clone();
            move || {
                let attempt_count = attempt_count.clone();
                async move {
                    let count = attempt_count.fetch_add(1, std::sync::atomic::Ordering::SeqCst);

                    if count < 2 {
                        // Fail first 2 attempts
                        Err(SftpError::IoError(std::io::Error::new(
                            std::io::ErrorKind::ConnectionReset,
                            "Connection reset",
                        )))
                    } else {
                        // Succeed on 3rd attempt
                        Ok(2048)
                    }
                }
            }
        };

        struct MockStore;
        #[async_trait::async_trait]
        impl ProgressStore for MockStore {
            async fn save(&self, _progress: &StoredTransferProgress) -> Result<(), SftpError> {
                Ok(())
            }
            async fn load(&self, _id: &str) -> Result<Option<StoredTransferProgress>, SftpError> {
                Ok(None)
            }
            async fn list_incomplete(
                &self,
                _id: &str,
            ) -> Result<Vec<StoredTransferProgress>, SftpError> {
                Ok(vec![])
            }
            async fn list_all_incomplete(&self) -> Result<Vec<StoredTransferProgress>, SftpError> {
                Ok(vec![])
            }
            async fn delete(&self, _id: &str) -> Result<(), SftpError> {
                Ok(())
            }
            async fn delete_for_session(&self, _id: &str) -> Result<(), SftpError> {
                Ok(())
            }
        }

        let progress = StoredTransferProgress::new(
            "test-2".to_string(),
            crate::sftp::progress::TransferType::Download,
            "/remote/file.txt".into(),
            "/local/file.txt".into(),
            2048,
            "session-1".to_string(),
        );

        let result = transfer_with_retry(
            transfer_fn,
            RetryConfig::new(3), // Allow up to 3 retries
            Arc::new(MockStore),
            progress,
            None, // No transfer control for retry test
        )
        .await;

        assert!(result.is_ok());
        assert_eq!(result.unwrap(), 2048);
        assert_eq!(
            attempt_count.load(std::sync::atomic::Ordering::SeqCst),
            3 // 2 failures + 1 success
        );
    }

    #[tokio::test]
    async fn test_transfer_with_retry_non_retryable_error() {
        let transfer_fn = || async move {
            // Non-retryable error
            Err(SftpError::PermissionDenied("Access denied".to_string()))
        };

        struct MockStore;
        #[async_trait::async_trait]
        impl ProgressStore for MockStore {
            async fn save(&self, _progress: &StoredTransferProgress) -> Result<(), SftpError> {
                Ok(())
            }
            async fn load(&self, _id: &str) -> Result<Option<StoredTransferProgress>, SftpError> {
                Ok(None)
            }
            async fn list_incomplete(
                &self,
                _id: &str,
            ) -> Result<Vec<StoredTransferProgress>, SftpError> {
                Ok(vec![])
            }
            async fn list_all_incomplete(&self) -> Result<Vec<StoredTransferProgress>, SftpError> {
                Ok(vec![])
            }
            async fn delete(&self, _id: &str) -> Result<(), SftpError> {
                Ok(())
            }
            async fn delete_for_session(&self, _id: &str) -> Result<(), SftpError> {
                Ok(())
            }
        }

        let progress = StoredTransferProgress::new(
            "test-3".to_string(),
            crate::sftp::progress::TransferType::Download,
            "/remote/file.txt".into(),
            "/local/file.txt".into(),
            2048,
            "session-1".to_string(),
        );

        let result = transfer_with_retry(
            transfer_fn,
            RetryConfig::default(),
            Arc::new(MockStore),
            progress,
            None, // No transfer control for non-retryable error test
        )
        .await;

        assert!(result.is_err());
        match result.unwrap_err() {
            SftpError::PermissionDenied(_) => {}
            other => panic!("Expected PermissionDenied error, got {:?}", other),
        }
    }
}
