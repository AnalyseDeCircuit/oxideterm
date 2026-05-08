// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use crate::filter::{TrzszFilter, TrzszFilterOutput};
use crate::types::TrzszTransferPolicy;
use crate::{TRZSZ_API_VERSION, TrzszCapabilitiesDto, TrzszError};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrzszControllerState {
    Active,
    Draining,
    Disposed,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TrzszCapabilitiesProbeResult {
    Available(TrzszCapabilitiesDto),
    Unavailable {
        reason: String,
        detail: Option<String>,
    },
}

impl Default for TrzszCapabilitiesProbeResult {
    fn default() -> Self {
        Self::Unavailable {
            reason: "invoke-failed".to_string(),
            detail: None,
        }
    }
}

#[derive(Debug)]
pub struct TrzszController {
    session_id: String,
    connection_id: String,
    runtime_id: String,
    owner_id: String,
    state: TrzszControllerState,
    capability_request_version: u64,
    capabilities: TrzszCapabilitiesProbeResult,
    capability_error: Option<TrzszError>,
    allow_cleanup_protocol: bool,
    filter: TrzszFilter,
}

impl TrzszController {
    pub fn new(
        session_id: impl Into<String>,
        connection_id: impl Into<String>,
        runtime_id: impl Into<String>,
        owner_id: impl Into<String>,
        transfer_policy: TrzszTransferPolicy,
    ) -> Self {
        Self {
            session_id: session_id.into(),
            connection_id: connection_id.into(),
            runtime_id: runtime_id.into(),
            owner_id: owner_id.into(),
            state: TrzszControllerState::Active,
            capability_request_version: 0,
            capabilities: TrzszCapabilitiesProbeResult::default(),
            capability_error: None,
            allow_cleanup_protocol: false,
            filter: TrzszFilter::new(transfer_policy),
        }
    }

    pub fn session_id(&self) -> &str {
        &self.session_id
    }

    pub fn connection_id(&self) -> &str {
        &self.connection_id
    }

    pub fn runtime_id(&self) -> &str {
        &self.runtime_id
    }

    pub fn owner_id(&self) -> &str {
        &self.owner_id
    }

    pub fn state(&self) -> TrzszControllerState {
        self.state
    }

    pub fn filter(&self) -> &TrzszFilter {
        &self.filter
    }

    pub fn filter_mut(&mut self) -> &mut TrzszFilter {
        &mut self.filter
    }

    pub fn matches_runtime(&self, connection_id: &str, runtime_id: &str) -> bool {
        self.connection_id == connection_id && self.runtime_id == runtime_id
    }

    pub fn set_terminal_columns(&mut self, columns: usize) {
        if columns > 0 {
            self.filter.set_terminal_columns(columns);
        }
    }

    pub fn update_transfer_policy(&mut self, transfer_policy: TrzszTransferPolicy) {
        self.filter.update_transfer_policy(transfer_policy);
    }

    pub fn get_capabilities(&self) -> &TrzszCapabilitiesProbeResult {
        &self.capabilities
    }

    pub fn refresh_capabilities(&mut self, result: TrzszCapabilitiesProbeResult) {
        if self.state == TrzszControllerState::Disposed {
            return;
        }
        self.capability_request_version += 1;
        self.capability_error = Self::validate_capabilities(&result);
        self.capabilities = result;
    }

    pub fn wait_for_transfer_ready(&self) -> Result<(), TrzszError> {
        if let Some(error) = &self.capability_error {
            return Err(clone_capability_error(error));
        }
        Ok(())
    }

    pub fn process_server_output(&mut self, output: &[u8]) -> Vec<TrzszFilterOutput> {
        if !self.can_process_io() {
            return Vec::new();
        }
        self.filter.process_server_output(output)
    }

    pub fn process_terminal_input(&mut self, input: &str) -> Option<TrzszFilterOutput> {
        if !self.can_process_io() {
            return None;
        }
        self.filter.process_terminal_input(input)
    }

    pub fn process_binary_input(&mut self, input: &str) -> Option<TrzszFilterOutput> {
        if !self.can_process_io() {
            return None;
        }
        self.filter.process_binary_input(input)
    }

    pub fn stop(&mut self) -> Vec<TrzszFilterOutput> {
        if self.state == TrzszControllerState::Disposed {
            return Vec::new();
        }
        self.state = TrzszControllerState::Draining;
        self.allow_cleanup_protocol = true;
        self.filter.dispose()
    }

    pub fn dispose(&mut self) -> Vec<TrzszFilterOutput> {
        if self.state == TrzszControllerState::Disposed {
            return Vec::new();
        }
        self.state = TrzszControllerState::Disposed;
        self.capability_request_version += 1;
        self.allow_cleanup_protocol = true;
        self.filter.dispose()
    }

    pub fn finish_cleanup_protocol(&mut self) {
        self.allow_cleanup_protocol = false;
    }

    pub fn can_send_cleanup_protocol(&self) -> bool {
        self.state == TrzszControllerState::Active || self.allow_cleanup_protocol
    }

    fn can_process_io(&self) -> bool {
        self.state == TrzszControllerState::Active
    }

    fn validate_capabilities(result: &TrzszCapabilitiesProbeResult) -> Option<TrzszError> {
        let TrzszCapabilitiesProbeResult::Available(capabilities) = result else {
            return None;
        };

        if capabilities.provider != "trzsz" || capabilities.api_version != TRZSZ_API_VERSION {
            return Some(TrzszError::InvalidApiVersion {
                expected: TRZSZ_API_VERSION,
                got: capabilities.api_version,
            });
        }

        None
    }
}

fn clone_capability_error(error: &TrzszError) -> TrzszError {
    match error {
        TrzszError::InvalidApiVersion { expected, got } => TrzszError::InvalidApiVersion {
            expected: *expected,
            got: *got,
        },
        _ => TrzszError::InvalidState(error.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::TrzszErrorCode;
    use crate::filter::TrzszFilterOutput;
    use crate::types::TrzszTransferPolicy;

    #[test]
    fn controller_forwards_io_while_active() {
        let mut controller = TrzszController::new(
            "session",
            "conn",
            "runtime",
            "owner",
            TrzszTransferPolicy::default(),
        );
        assert_eq!(
            controller.process_server_output(b"abc"),
            vec![TrzszFilterOutput::WriteTerminal(b"abc".to_vec())]
        );
        assert_eq!(
            controller.process_terminal_input("ls\r"),
            Some(TrzszFilterOutput::SendServer(b"ls\r".to_vec()))
        );
    }

    #[test]
    fn controller_blocks_io_after_stop_but_allows_cleanup_protocol() {
        let mut controller = TrzszController::new(
            "session",
            "conn",
            "runtime",
            "owner",
            TrzszTransferPolicy::default(),
        );
        let _ = controller.stop();
        assert_eq!(controller.state(), TrzszControllerState::Draining);
        assert!(controller.can_send_cleanup_protocol());
        assert!(controller.process_server_output(b"abc").is_empty());
        assert_eq!(controller.process_terminal_input("ls\r"), None);
    }

    #[test]
    fn controller_rejects_capability_api_mismatch() {
        let mut controller = TrzszController::new(
            "session",
            "conn",
            "runtime",
            "owner",
            TrzszTransferPolicy::default(),
        );
        let mut capabilities = TrzszCapabilitiesDto::default();
        capabilities.api_version = 2;
        controller.refresh_capabilities(TrzszCapabilitiesProbeResult::Available(capabilities));
        let error = controller.wait_for_transfer_ready().unwrap_err();
        assert_eq!(error.code(), TrzszErrorCode::InvalidApiVersion);
    }
}
