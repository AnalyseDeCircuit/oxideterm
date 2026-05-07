// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use std::time::Duration;

use super::SftpError;

#[derive(Debug, Clone)]
pub struct RetryConfig {
    pub max_retries: usize,
    pub initial_backoff_secs: u64,
    pub backoff_multiplier: f64,
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

pub fn calculate_backoff(attempt: usize, config: &RetryConfig) -> Duration {
    let delay_secs = (config.initial_backoff_secs as f64
        * config.backoff_multiplier.powi(attempt as i32))
    .min(config.max_backoff_secs as f64);
    Duration::from_secs(delay_secs as u64)
}

pub fn is_retryable_error(error: &SftpError) -> bool {
    match error {
        SftpError::IoError(_) | SftpError::ChannelError(_) | SftpError::TransferError(_) => true,
        SftpError::ProtocolError(message) => {
            message.contains("timeout") || message.contains("connection")
        }
        _ => false,
    }
}
