// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

mod model;
#[cfg(not(target_family = "wasm"))]
mod runtime;
mod validation;

#[cfg(not(target_family = "wasm"))]
pub use model::BackgroundTaskExecution;
pub use model::{
    BackgroundTaskCondition, BackgroundTaskEvent, BackgroundTaskExecutionResult, BackgroundTaskId,
    BackgroundTaskMode, BackgroundTaskOwner, BackgroundTaskSnapshot, BackgroundTaskSpec,
    BackgroundTaskState, fingerprint_json,
};
#[cfg(not(target_family = "wasm"))]
pub use runtime::{BackgroundTaskExecutor, BackgroundTaskRuntime};
pub use validation::{
    BackgroundTaskLimits, BackgroundTaskValidationError, validate_background_task_spec,
};
