// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use oxideterm_connections::{ConnectionInfo, ConnectionStore};
use serde::Serialize;

use crate::{
    args::{
        ConnectionSearchArgs, ConnectionShowArgs, ConnectionsAction, ConnectionsCommand, JsonArgs,
    },
    connections_validate,
    error::{CliError, CliResult, runtime_error},
    output::{self, OutputFormat},
    paths::default_connections_path,
};

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ConnectionsListResponse {
    path: String,
    count: usize,
    connections: Vec<ConnectionInfo>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ConnectionGroupsResponse {
    path: String,
    count: usize,
    groups: Vec<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ConnectionShowResponse {
    path: String,
    connection: ConnectionInfo,
}

pub fn run(command: ConnectionsCommand) -> CliResult<()> {
    match command.action {
        ConnectionsAction::List(args) => list_connections(args),
        ConnectionsAction::Show(args) => show_connection(args),
        ConnectionsAction::Groups(args) => list_groups(args),
        ConnectionsAction::Search(args) => search_connections(args),
        ConnectionsAction::Export(args) => export_connections(args),
        ConnectionsAction::Validate(args) => connections_validate::run(args),
    }
}

fn list_connections(args: JsonArgs) -> CliResult<()> {
    let store = load_connection_store(args.json)?;
    let connections = store.connection_infos();
    match output::format_from_flag(args.json) {
        OutputFormat::Json => output::write_json(&ConnectionsListResponse {
            path: store.path().display().to_string(),
            count: connections.len(),
            connections,
        }),
        OutputFormat::Text => {
            if connections.is_empty() {
                output::write_text("No saved connections");
            } else {
                for connection in connections {
                    output::write_text(format_connection_row(&connection));
                }
            }
            Ok(())
        }
    }
}

fn list_groups(args: JsonArgs) -> CliResult<()> {
    let store = load_connection_store(args.json)?;
    let groups = store.groups().to_vec();
    match output::format_from_flag(args.json) {
        OutputFormat::Json => output::write_json(&ConnectionGroupsResponse {
            path: store.path().display().to_string(),
            count: groups.len(),
            groups,
        }),
        OutputFormat::Text => {
            if groups.is_empty() {
                output::write_text("No connection groups");
            } else {
                for group in groups {
                    output::write_text(group);
                }
            }
            Ok(())
        }
    }
}

fn search_connections(args: ConnectionSearchArgs) -> CliResult<()> {
    let store = load_connection_store(args.json)?;
    let connections = filter_connections(&store.connection_infos(), &args.query);
    match output::format_from_flag(args.json) {
        OutputFormat::Json => output::write_json(&ConnectionsListResponse {
            path: store.path().display().to_string(),
            count: connections.len(),
            connections,
        }),
        OutputFormat::Text => {
            if connections.is_empty() {
                output::write_text("No matching connections");
            } else {
                for connection in connections {
                    output::write_text(format_connection_row(&connection));
                }
            }
            Ok(())
        }
    }
}

fn show_connection(args: ConnectionShowArgs) -> CliResult<()> {
    let store = load_connection_store(args.json)?;
    let connections = store.connection_infos();
    let Some(connection) = find_connection(&connections, &args.query) else {
        return Err(CliError::new(
            "connection_not_found",
            format!("connection '{}' was not found", args.query),
            args.json,
        ));
    };
    match output::format_from_flag(args.json) {
        OutputFormat::Json => output::write_json(&ConnectionShowResponse {
            path: store.path().display().to_string(),
            connection,
        }),
        OutputFormat::Text => {
            output::write_text(format_connection_details(&connection));
            Ok(())
        }
    }
}

fn export_connections(args: JsonArgs) -> CliResult<()> {
    let store = load_connection_store(args.json)?;
    // Export the sync snapshot instead of raw store data so credential material stays out of CLI output.
    let snapshot = store
        .export_saved_connections_snapshot()
        .map_err(|error| runtime_error(error, args.json))?;
    match output::format_from_flag(args.json) {
        OutputFormat::Json => output::write_json(&serde_json::json!({
            "path": store.path().display().to_string(),
            "revision": snapshot.revision,
            "exportedAt": snapshot.exported_at,
            "count": snapshot.records.len(),
            "snapshot": snapshot,
        })),
        OutputFormat::Text => {
            output::write_text(serde_json::to_string_pretty(&snapshot).map_err(|error| {
                CliError::new("serialization_failed", error.to_string(), args.json)
            })?);
            Ok(())
        }
    }
}

fn load_connection_store(json: bool) -> CliResult<ConnectionStore> {
    ConnectionStore::load_read_only(default_connections_path())
        .map_err(|error| runtime_error(error, json))
}

fn filter_connections(connections: &[ConnectionInfo], query: &str) -> Vec<ConnectionInfo> {
    let query = query.trim().to_ascii_lowercase();
    if query.is_empty() {
        return connections.to_vec();
    }
    connections
        .iter()
        .filter(|connection| {
            connection.id.to_ascii_lowercase().contains(&query)
                || connection.name.to_ascii_lowercase().contains(&query)
                || connection.host.to_ascii_lowercase().contains(&query)
                || connection.username.to_ascii_lowercase().contains(&query)
                || connection
                    .group
                    .as_ref()
                    .is_some_and(|group| group.to_ascii_lowercase().contains(&query))
                || connection
                    .tags
                    .iter()
                    .any(|tag| tag.to_ascii_lowercase().contains(&query))
        })
        .cloned()
        .collect()
}

fn find_connection(connections: &[ConnectionInfo], query: &str) -> Option<ConnectionInfo> {
    connections
        .iter()
        .find(|connection| connection.id == query || connection.name == query)
        .cloned()
        .or_else(|| {
            let lower = query.to_ascii_lowercase();
            connections
                .iter()
                .find(|connection| connection.name.to_ascii_lowercase() == lower)
                .cloned()
        })
}

fn format_connection_row(connection: &ConnectionInfo) -> String {
    format!(
        "{}\t{}@{}:{}\t{}",
        connection.name, connection.username, connection.host, connection.port, connection.id
    )
}

fn format_connection_details(connection: &ConnectionInfo) -> String {
    let group = connection.group.as_deref().unwrap_or("-");
    let tags = if connection.tags.is_empty() {
        "-".to_string()
    } else {
        connection.tags.join(",")
    };
    format!(
        "id: {}\nname: {}\ngroup: {}\nhost: {}\nport: {}\nusername: {}\nauth: {:?}\ntags: {}",
        connection.id,
        connection.name,
        group,
        connection.host,
        connection.port,
        connection.username,
        connection.auth_type,
        tags
    )
}

#[cfg(test)]
mod tests {
    use oxideterm_connections::AuthType;

    use super::*;

    fn sample_connection(id: &str, name: &str) -> ConnectionInfo {
        ConnectionInfo {
            id: id.to_string(),
            name: name.to_string(),
            group: Some("prod".to_string()),
            host: "example.com".to_string(),
            port: 22,
            username: "root".to_string(),
            auth_type: AuthType::Password,
            key_path: None,
            cert_path: None,
            proxy_chain: Vec::new(),
            created_at: "2026-05-26T00:00:00Z".to_string(),
            last_used_at: None,
            color: None,
            tags: vec!["primary".to_string()],
            agent_forwarding: false,
            post_connect_command: None,
        }
    }

    #[test]
    fn finds_connection_by_id_or_case_insensitive_name() {
        let connections = vec![sample_connection("id-1", "Prod")];

        assert_eq!(find_connection(&connections, "id-1").unwrap().name, "Prod");
        assert_eq!(find_connection(&connections, "prod").unwrap().id, "id-1");
        assert!(find_connection(&connections, "missing").is_none());
    }

    #[test]
    fn filters_connections_by_common_fields() {
        let connections = vec![
            sample_connection("id-1", "Prod"),
            ConnectionInfo {
                host: "staging.example.com".to_string(),
                group: Some("stage".to_string()),
                tags: vec!["preview".to_string()],
                ..sample_connection("id-2", "Staging")
            },
        ];

        assert_eq!(filter_connections(&connections, "primary").len(), 1);
        assert_eq!(filter_connections(&connections, "stage")[0].name, "Staging");
        assert_eq!(filter_connections(&connections, "example.com").len(), 2);
        assert!(filter_connections(&connections, "missing").is_empty());
    }
}
