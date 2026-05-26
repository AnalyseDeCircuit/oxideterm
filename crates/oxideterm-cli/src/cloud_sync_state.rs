// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use oxideterm_cloud_sync::state::CloudSyncPersistedState;
use serde::Serialize;

use crate::{
    args::{CloudSyncStateAction, CloudSyncStateCommand},
    cloud_sync_preview,
    error::{CliError, CliResult},
    json_query,
    output::{self, OutputFormat},
    paths::default_cloud_sync_path,
};

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct CloudSyncStateResponse {
    path: String,
    state: CloudSyncPersistedState,
}

pub fn run(command: CloudSyncStateCommand) -> CliResult<()> {
    match command.action {
        CloudSyncStateAction::Show(args) => show_state(args.json),
        CloudSyncStateAction::Get(args) => get_state_value(args.key, args.json),
    }
}

fn show_state(json: bool) -> CliResult<()> {
    let path = default_cloud_sync_path();
    let state = cloud_sync_preview::load_persisted_state(&path, json)?;
    match output::format_from_flag(json) {
        OutputFormat::Json => output::write_json(&CloudSyncStateResponse {
            path: path.display().to_string(),
            state,
        }),
        OutputFormat::Text => {
            let value = serde_json::to_value(&state)
                .map_err(|error| CliError::new("serialization_failed", error.to_string(), json))?;
            output::write_text(
                serde_json::to_string_pretty(&value).map_err(|error| {
                    CliError::new("serialization_failed", error.to_string(), json)
                })?,
            );
            Ok(())
        }
    }
}

fn get_state_value(key: String, json: bool) -> CliResult<()> {
    let path = default_cloud_sync_path();
    let state = cloud_sync_preview::load_persisted_state(&path, json)?;
    let value = serde_json::to_value(&state)
        .map_err(|error| CliError::new("serialization_failed", error.to_string(), json))?;
    let Some(found) = json_query::value_at_path(&value, &key) else {
        return Err(CliError::new(
            "cloud_sync_state_key_not_found",
            format!("cloud sync state key '{key}' was not found"),
            json,
        ));
    };

    match output::format_from_flag(json) {
        OutputFormat::Json => output::write_json(found),
        OutputFormat::Text => {
            output::write_text(json_query::value_to_text(found));
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use serde_json::Value;

    use super::*;

    #[test]
    fn state_value_can_be_read_with_dotted_path() {
        let state = CloudSyncPersistedState::default();
        let value = serde_json::to_value(state).unwrap();

        assert_eq!(
            json_query::value_at_path(&value, "settings.namespace"),
            Some(&Value::String("default".to_string()))
        );
    }
}
