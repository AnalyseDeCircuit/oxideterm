// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only
use crate::{
    args::{
        CloudSyncApplyArgs, CloudSyncPullArgs, CloudSyncResolveArgs, CloudSyncResolveStrategy,
        CloudSyncWriteArgs, WriteArgs,
    },
    error::{CliError, CliResult, runtime_error},
    output,
    paths::{default_cloud_sync_path, default_connections_path, default_forwards_path},
    settings, write_guard,
};
use oxideterm_cloud_sync::{
    operation::CloudSyncOperationService, secrets::CloudSyncKeychainSecretProvider,
    state::CloudSyncStateStore,
};
use oxideterm_connections::ConnectionStore;
use oxideterm_forwarding::{ForwardingRegistry, SavedForwardStore};
use oxideterm_settings::SettingsStore;
pub(crate) fn synchronize(args: CloudSyncWriteArgs) -> CliResult<()> {
    synchronize_with_choice(args, false, None)
}

pub(crate) fn change_password(args: CloudSyncWriteArgs) -> CliResult<()> {
    let password = crate::cloud_sync_secrets::read_secret_value(true, None, args.write.json)?;
    if password.chars().count() < 6 {
        return Err(CliError::new(
            "password_too_short",
            "Use at least six characters for the sync password",
            args.write.json,
        ));
    }
    synchronize_with_choice(args, false, Some(password))
}

fn synchronize_with_choice(
    args: CloudSyncWriteArgs,
    prefer_local: bool,
    password: Option<zeroize::Zeroizing<String>>,
) -> CliResult<()> {
    use fs2::FileExt;
    let write = effective_cloud_sync_write(args.write);
    let json = write.json;
    let directory = std::path::PathBuf::from(settings::load_settings_read_only(json)?.path);
    let directory = directory
        .parent()
        .ok_or_else(|| CliError::new("settings_path", "Settings directory is unavailable", json))?;
    std::fs::create_dir_all(directory).map_err(|error| runtime_error(error, json))?;
    // Match the desktop's single-instance files and hold every channel lock for
    // the entire operation, including recovery, so a live owner cannot overwrite it.
    let mut locks = Vec::new();
    for channel in ["stable", "beta", "development"] {
        let file = std::fs::OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(directory.join(format!("oxideterm-native-instance-{channel}.lock")))
            .map_err(|error| runtime_error(error, json))?;
        file.try_lock_exclusive().map_err(|_| {
            CliError::new(
                "cloud_sync_app_running",
                "Close the desktop app before synchronizing from the CLI",
                json,
            )
        })?;
        locks.push(file);
    }
    oxideterm_portable_runtime::acquire_portable_instance_lock()
        .map_err(|error| runtime_error(error, json))?;
    let mut state = load_state_store(json)?;
    let mut connections = load_connection_store(json)?;
    let mut settings = load_settings_store(json)?;
    let mut provider = CloudSyncKeychainSecretProvider::new(state.state().secret_hints.clone());
    let recovered = if write.dry_run {
        0
    } else {
        oxideterm_cloud_sync::sync_v3::RecoveryJournal::recover_pending(
            &mut connections,
            &mut settings,
            &default_forwards_path(),
            &mut provider,
        )
        .map_err(|error| runtime_error(error, json))?
    };
    let _guard = write_guard::prepare_write(&write, true)?;
    let forwards = load_forwarding_registry(json)?;
    let runtime = runtime(json)?;
    let service = CloudSyncOperationService::new();
    let mut prepared = runtime
        .block_on(service.prepare_sync(
            &mut connections,
            &forwards,
            &mut settings,
            &state.state().settings,
            &mut provider,
            state.state().sync_scope(&[]),
            Default::default(),
        ))
        .map_err(|error| runtime_error(error, json))?;
    if prefer_local {
        prepared
            .choose_local_conflicts()
            .map_err(|error| runtime_error(error, json))?;
    }
    if write.dry_run {
        let summary = prepared
            .summary()
            .map_err(|error| runtime_error(error, json))?;
        return if json {
            output::write_json(&summary)
        } else {
            println!(
                "Changed fields: {}; conflicts: {}; upgrading: {}",
                summary.changed_fields,
                summary.conflicts.len(),
                summary.upgrading
            );
            Ok(())
        };
    }
    let applied = prepared
        .apply(&mut connections, &forwards, &mut settings)
        .map_err(|error| runtime_error(error, json))?;
    let result = if let Some(password) = password {
        runtime.block_on(applied.change_password(password, &mut provider, settings.path()))
    } else {
        runtime.block_on(applied.publish())
    };
    state.state_mut().secret_hints = provider.hints().clone();
    state.save().map_err(|error| runtime_error(error, json))?;
    let mut outcome = result.map_err(|error| runtime_error(error, json))?;
    outcome.recovered |= recovered > 0;
    oxideterm_cloud_sync::state_transitions::finish_causal_sync_state(
        state.state_mut(),
        &outcome,
        chrono::Utc::now().to_rfc3339(),
    );
    state.save().map_err(|error| runtime_error(error, json))?;
    if json {
        output::write_json(&outcome)
    } else {
        println!(
            "Applied: {}; published: {}; recovered: {}; conflicts: {}; cleanup pending: {}",
            outcome.applied,
            outcome.published,
            outcome.recovered,
            outcome.conflicts.len(),
            outcome.cleanup_pending
        );
        Ok(())
    }
}

pub(crate) fn push(args: CloudSyncWriteArgs) -> CliResult<()> {
    synchronize(args)
}
pub(crate) fn pull(args: CloudSyncPullArgs) -> CliResult<()> {
    require_merge(args.strategy, args.write.json)?;
    synchronize(CloudSyncWriteArgs { write: args.write })
}
pub(crate) fn apply(args: CloudSyncApplyArgs) -> CliResult<()> {
    require_merge(args.strategy, args.write.json)?;
    if args.from == crate::args::CloudSyncApplySource::Local {
        return Err(CliError::new(
            "cloud_sync_strategy",
            "Use cloud-sync resolve --strategy local-wins to choose local conflict candidates",
            args.write.json,
        ));
    }
    synchronize(CloudSyncWriteArgs { write: args.write })
}
fn require_merge(
    strategy: Option<crate::args::CloudSyncConflictStrategy>,
    json: bool,
) -> CliResult<()> {
    if strategy.is_some_and(|strategy| strategy != crate::args::CloudSyncConflictStrategy::Merge) {
        return Err(CliError::new(
            "cloud_sync_strategy",
            "Causal sync merges independent changes; choose individual conflicts in the desktop preview",
            json,
        ));
    }
    Ok(())
}
pub(crate) fn resolve(args: CloudSyncResolveArgs) -> CliResult<()> {
    if matches!(args.strategy, CloudSyncResolveStrategy::RemoteWins) {
        return Err(CliError::new(
            "cloud_sync_conflict_choice_required",
            "Choose individual remote candidates in the desktop sync preview",
            args.write.json,
        ));
    }
    synchronize_with_choice(CloudSyncWriteArgs { write: args.write }, true, None)
}

fn effective_cloud_sync_write(mut write: WriteArgs) -> WriteArgs {
    if !write.yes {
        write.dry_run = true;
    }
    write
}

fn load_state_store(json: bool) -> CliResult<CloudSyncStateStore> {
    CloudSyncStateStore::load(default_cloud_sync_path()).map_err(|error| runtime_error(error, json))
}

fn load_connection_store(json: bool) -> CliResult<ConnectionStore> {
    ConnectionStore::load(default_connections_path()).map_err(|error| runtime_error(error, json))
}

fn load_forwarding_registry(json: bool) -> CliResult<ForwardingRegistry> {
    let store = SavedForwardStore::load(default_forwards_path())
        .map_err(|error| runtime_error(error, json))?;
    Ok(ForwardingRegistry::new_with_store(store))
}

fn load_settings_store(json: bool) -> CliResult<SettingsStore> {
    let read_only = settings::load_settings_read_only(json)?;
    Ok(SettingsStore::from_read_only(
        read_only.path,
        read_only.settings,
    ))
}

fn runtime(json: bool) -> CliResult<tokio::runtime::Runtime> {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|error| CliError::new("runtime_error", error.to_string(), json))
}
