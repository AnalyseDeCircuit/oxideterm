// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use std::fs;

use oxideterm_connections::{
    BatchDuplicateNamePolicy, BatchExistingConnection, BatchIssue, BatchPreview, BatchRowStatus,
    SavedAuth, SecretString, apply_batch, parse_batch_data, parse_batch_template,
    resolve_batch_rows,
};
use serde::Deserialize;

use crate::{
    args::{ConnectionBatchCreateArgs, ConnectionBatchDuplicateArg},
    error::{CliError, CliResult},
};

use super::spec::read_direct_secret;
use super::{ConnectionsChange, finish_connections_write, load_connection_store};

#[derive(Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct BatchSpec {
    template: Option<String>,
    data: Option<String>,
    duplicate: Option<String>,
}

/// Secrets handed in through flags stay inside this struct until the engine
/// materializes them into the protected store.
pub(super) struct BatchAuthInput {
    pub(super) password: Option<SecretString>,
    pub(super) key_path: Option<String>,
}

pub(super) fn batch_create(args: ConnectionBatchCreateArgs) -> CliResult<i32> {
    let json = args.write.json;
    let (template_text, data_text, duplicate, auth_input) = batch_input(&args, json)?;
    let template = parse_batch_template(&template_text).map_err(|issue| {
        batch_issue_error(
            "connection_batch_template_invalid",
            "template",
            &issue,
            json,
        )
    })?;
    let data = parse_batch_data(&data_text).map_err(|issue| {
        batch_issue_error("connection_batch_data_invalid", "data", &issue, json)
    })?;
    let default_auth = resolve_batch_default_auth(&template, &auth_input, json)?;

    let store = load_connection_store(json)?;
    let existing = store
        .connections()
        .iter()
        .map(BatchExistingConnection::from_saved)
        .collect::<Vec<_>>();
    let preview = resolve_batch_rows(
        &template,
        &data,
        default_auth.as_ref(),
        &existing,
        duplicate,
    );

    let changes = batch_changes(&preview);
    ensure_rows_valid(&preview, args.write.dry_run, json)?;
    let addable = preview.addable_rows();
    finish_connections_write(args.write, changes, |store| {
        apply_batch(store, addable).map(|_| ())
    })
}

fn batch_input(
    args: &ConnectionBatchCreateArgs,
    json: bool,
) -> CliResult<(String, String, BatchDuplicateNamePolicy, BatchAuthInput)> {
    let auth_input = BatchAuthInput {
        password: read_direct_secret(
            args.password_stdin,
            args.password_env.as_deref(),
            "password",
            json,
        )?,
        key_path: args.key_path.clone().filter(|path| !path.trim().is_empty()),
    };
    if let Some(path) = &args.spec_path {
        if args.template_path.is_some() || args.data_path.is_some() {
            return Err(CliError::new(
                "connection_batch_input_conflict",
                "use either --spec or --template-file/--data-file, not both",
                json,
            ));
        }
        let spec: BatchSpec =
            serde_json::from_str(&read_text_file(path, "spec", json)?).map_err(|error| {
                CliError::new(
                    "connection_batch_spec_invalid",
                    format!("failed to parse batch spec {path}: {error}"),
                    json,
                )
            })?;
        let template = spec.template.ok_or_else(|| {
            CliError::new(
                "connection_batch_spec_invalid",
                "batch spec is missing the template text",
                json,
            )
        })?;
        let data = spec.data.ok_or_else(|| {
            CliError::new(
                "connection_batch_spec_invalid",
                "batch spec is missing the data text",
                json,
            )
        })?;
        let duplicate = match spec.duplicate.as_deref() {
            Some(value) => {
                duplicate_policy(ConnectionBatchDuplicateArg::parse(value).ok_or_else(|| {
                    CliError::new(
                        "connection_batch_spec_invalid",
                        format!("unknown duplicate policy \"{value}\"; expected skip or rename"),
                        json,
                    )
                })?)
            }
            None => duplicate_policy(args.duplicate),
        };
        return Ok((template, data, duplicate, auth_input));
    }
    let template_path = args.template_path.as_ref().ok_or_else(|| {
        CliError::new(
            "connection_batch_input_missing",
            "provide --spec or --template-file and --data-file",
            json,
        )
    })?;
    let data_path = args.data_path.as_ref().ok_or_else(|| {
        CliError::new(
            "connection_batch_input_missing",
            "provide --spec or --template-file and --data-file",
            json,
        )
    })?;
    let template = read_text_file(template_path, "template", json)?;
    let data = read_text_file(data_path, "data", json)?;
    Ok((template, data, duplicate_policy(args.duplicate), auth_input))
}

/// The dialog authentication only applies when the template does not carry an
/// identity path; mixing both sources would make the winning one ambiguous.
pub(super) fn resolve_batch_default_auth(
    template: &oxideterm_connections::BatchTemplate,
    auth_input: &BatchAuthInput,
    json: bool,
) -> CliResult<Option<SavedAuth>> {
    let template_has_identity =
        template.identity_file.is_some() || template.certificate_file.is_some();
    if template_has_identity {
        if auth_input.password.is_some() || auth_input.key_path.is_some() {
            return Err(CliError::new(
                "connection_batch_auth_conflict",
                "the template already provides IdentityFile/CertificateFile; \
                 do not pass --password-env, --password-stdin or --key-path",
                json,
            ));
        }
        return Ok(None);
    }
    match (&auth_input.password, &auth_input.key_path) {
        (Some(_), Some(_)) => Err(CliError::new(
            "connection_batch_auth_conflict",
            "--password-env/--password-stdin and --key-path are mutually exclusive",
            json,
        )),
        (Some(password), None) => Ok(Some(SavedAuth::Password {
            empty_password: false,
            keychain_id: None,
            plaintext_password: Some(password.clone()),
        })),
        (None, Some(key_path)) => Ok(Some(SavedAuth::Key {
            key_path: key_path.clone(),
            has_passphrase: false,
            passphrase_keychain_id: None,
            plaintext_passphrase: None,
        })),
        (None, None) => Ok(None),
    }
}

/// A partial write would silently drop the rejected lines; the batch is one
/// transaction, so a real write is refused while any row is invalid. A dry run
/// still reports them.
pub(super) fn ensure_rows_valid(
    preview: &BatchPreview,
    dry_run: bool,
    json: bool,
) -> CliResult<()> {
    if dry_run || preview.issues.is_empty() {
        return Ok(());
    }
    let lines = preview
        .issues
        .iter()
        .map(|issue| format!("{} ({})", issue.line, issue.kind))
        .collect::<Vec<_>>()
        .join(", ");
    Err(CliError::new(
        "connection_batch_rows_invalid",
        &format!(
            "{} row(s) are invalid: {lines}; fix them or run --dry-run to inspect",
            preview.issues.len()
        ),
        json,
    ))
}

pub(super) fn batch_changes(preview: &BatchPreview) -> Vec<ConnectionsChange> {
    let mut changes = Vec::new();
    for row in &preview.rows {
        let after = format!("{}@{}:{}", row.username, row.host, row.port);
        changes.push(ConnectionsChange {
            action: match row.status {
                BatchRowStatus::Add | BatchRowStatus::Renamed => "batchCreate",
                BatchRowStatus::Update => "batchUpdate",
                BatchRowStatus::Unchanged => "batchUnchanged",
                BatchRowStatus::SkippedDuplicateTarget => "batchSkipDuplicateTarget",
                BatchRowStatus::SkippedExistingName => "batchSkipExistingName",
            },
            target: row.name.clone(),
            before: None,
            after: Some(after),
        });
    }
    for issue in &preview.issues {
        changes.push(ConnectionsChange {
            action: "batchInvalid",
            target: format!("line {}", issue.line),
            before: None,
            after: Some(issue.kind.to_string()),
        });
    }
    changes
}

fn duplicate_policy(value: ConnectionBatchDuplicateArg) -> BatchDuplicateNamePolicy {
    match value {
        ConnectionBatchDuplicateArg::Skip => BatchDuplicateNamePolicy::Skip,
        ConnectionBatchDuplicateArg::Rename => BatchDuplicateNamePolicy::Rename,
    }
}

fn read_text_file(path: &str, label: &str, json: bool) -> CliResult<String> {
    fs::read_to_string(path).map_err(|error| {
        CliError::new(
            "connection_batch_read_failed",
            format!("failed to read batch {label} {path}: {error}"),
            json,
        )
    })
}

fn batch_issue_error(code: &'static str, label: &str, issue: &BatchIssue, json: bool) -> CliError {
    CliError::new(
        code,
        format!("{} line {}: {}", label, issue.line, issue.kind),
        json,
    )
}
