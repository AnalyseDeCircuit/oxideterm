// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use std::{collections::HashMap, time::Duration};

use serde_json::Value;

use crate::{
    error::PluginError,
    event::PluginEvent,
    message::{PluginOutboundMessage, PluginRegistration, PluginRuntimeLogLevel},
    runtime_state::{PluginRuntimeHealth, PluginRuntimeLifecycleState, PluginRuntimeLogEntry},
};

const DEFAULT_RUNTIME_MAX_ERROR_COUNT: u32 = 3;

#[derive(Clone, Debug)]
pub struct PluginRuntimeSupervisorState {
    plugin_id: String,
    state: PluginRuntimeLifecycleState,
    lifecycle_timeout: Duration,
    max_error_count: u32,
    error_count: u32,
    last_error: Option<PluginError>,
    registrations: HashMap<String, PluginRegistration>,
    logs: Vec<PluginRuntimeLogEntry>,
}

impl PluginRuntimeSupervisorState {
    pub fn new(plugin_id: impl Into<String>, lifecycle_timeout: Duration) -> Self {
        Self {
            plugin_id: plugin_id.into(),
            state: PluginRuntimeLifecycleState::Inactive,
            lifecycle_timeout,
            max_error_count: DEFAULT_RUNTIME_MAX_ERROR_COUNT,
            error_count: 0,
            last_error: None,
            registrations: HashMap::new(),
            logs: Vec::new(),
        }
    }

    pub fn state(&self) -> PluginRuntimeLifecycleState {
        self.state
    }

    pub fn lifecycle_timeout(&self) -> Duration {
        self.lifecycle_timeout
    }

    pub fn health(&self) -> PluginRuntimeHealth {
        PluginRuntimeHealth {
            state: self.state,
            healthy: matches!(self.state, PluginRuntimeLifecycleState::Active),
            error_count: self.error_count,
        }
    }

    pub fn start_activation(&mut self) {
        self.state = PluginRuntimeLifecycleState::Activating;
    }

    pub fn mark_active(&mut self) {
        self.state = PluginRuntimeLifecycleState::Active;
        self.error_count = 0;
        self.last_error = None;
    }

    pub fn start_deactivation(&mut self) {
        self.state = PluginRuntimeLifecycleState::Deactivating;
    }

    pub fn kill(&mut self) {
        self.state = PluginRuntimeLifecycleState::Killed;
        self.dispose_all_registrations();
    }

    pub fn record_registration(&mut self, registration: PluginRegistration) -> Result<(), String> {
        if registration.plugin_id != self.plugin_id {
            return Err(format!(
                "Registration \"{}\" belongs to plugin \"{}\", expected \"{}\"",
                registration.registration_id, registration.plugin_id, self.plugin_id
            ));
        }
        self.registrations
            .insert(registration.registration_id.clone(), registration);
        Ok(())
    }

    pub fn dispose_all_registrations(&mut self) -> usize {
        let count = self.registrations.len();
        self.registrations.clear();
        count
    }

    pub fn dispose_registration(&mut self, registration_id: &str) -> bool {
        self.registrations.remove(registration_id).is_some()
    }

    pub fn registration_count(&self) -> usize {
        self.registrations.len()
    }

    pub fn record_log(&mut self, level: PluginRuntimeLogLevel, message: impl Into<String>) {
        self.logs.push(PluginRuntimeLogEntry {
            level,
            message: message.into(),
        });
    }

    pub fn log_count(&self) -> usize {
        self.logs.len()
    }

    pub fn record_error(&mut self, error: PluginError) {
        self.error_count = self.error_count.saturating_add(1);
        self.last_error = Some(error);
        // Repeatedly failing plugins are isolated at the supervisor boundary so
        // every runtime backend shares the same user-visible safety behavior.
        if self.error_count >= self.max_error_count {
            self.state = PluginRuntimeLifecycleState::AutoDisabled;
            self.dispose_all_registrations();
        } else {
            self.state = PluginRuntimeLifecycleState::Error;
        }
    }

    pub fn handle_outbound_message(
        &mut self,
        message: PluginOutboundMessage,
    ) -> Result<PluginOutboundEffect, PluginError> {
        match message {
            PluginOutboundMessage::RegisterContribution { registration } => {
                self.record_registration(registration)
                    .map_err(|error| PluginError::protocol("invalid_registration", error))?;
                Ok(PluginOutboundEffect::RegistrationChanged)
            }
            PluginOutboundMessage::DisposeContribution { registration_id } => {
                self.dispose_registration(&registration_id);
                Ok(PluginOutboundEffect::RegistrationChanged)
            }
            PluginOutboundMessage::Log { level, message } => {
                self.record_log(level, message);
                Ok(PluginOutboundEffect::None)
            }
            PluginOutboundMessage::RuntimeReady => {
                self.mark_active();
                Ok(PluginOutboundEffect::LifecycleChanged)
            }
            PluginOutboundMessage::RuntimeError { error } => {
                self.record_error(error);
                Ok(PluginOutboundEffect::LifecycleChanged)
            }
            PluginOutboundMessage::ReportProgress {
                registration_id,
                value,
            } => Ok(PluginOutboundEffect::Progress {
                registration_id,
                value,
            }),
            PluginOutboundMessage::EmitEvent { event } => Ok(PluginOutboundEffect::Event(event)),
            PluginOutboundMessage::CallHostApi {
                request_id,
                namespace,
                method,
                args,
            } => Ok(PluginOutboundEffect::HostCall {
                request_id,
                namespace,
                method,
                args,
            }),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum PluginOutboundEffect {
    None,
    RegistrationChanged,
    LifecycleChanged,
    Progress {
        registration_id: String,
        value: Value,
    },
    Event(PluginEvent),
    HostCall {
        request_id: String,
        namespace: String,
        method: String,
        args: Value,
    },
}
