use std::collections::{HashMap, hash_map::DefaultHasher};
use std::hash::{Hash, Hasher};

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResourceDockerContainer {
    pub id: String,
    pub name: String,
    pub image: String,
    pub state: String,
    pub status: String,
    pub ports: Option<String>,
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ResourceDockerStatus {
    #[default]
    Unknown,
    Available,
    Unavailable,
    Error {
        message: String,
    },
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResourceDockerSnapshot {
    pub status: ResourceDockerStatus,
    pub containers: Vec<ResourceDockerContainer>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DockerActionKind {
    Start,
    Stop,
    Restart,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DockerActionCommand {
    pub command: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DockerCaptureCommand {
    pub command: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct DockerInspectOverride {
    id: String,
    name: Option<String>,
    image: Option<String>,
}

const DOCKER_UNAVAILABLE_MARKER: &str = "__OXIDE_DOCKER_UNAVAILABLE__";
const DOCKER_ERROR_MARKER: &str = "__OXIDE_DOCKER_ERROR__";

const DOCKER_SAMPLE_COMMAND_UNIX: &str = concat!(
    "echo '===DOCKER==='; ",
    "if command -v docker >/dev/null 2>&1; then ",
    "oxide_docker_ps=$(docker ps -a --no-trunc --format '",
    "{{json .}}",
    "' 2>&1); ",
    "oxide_docker_status=$?; ",
    "if [ \"$oxide_docker_status\" -ne 0 ]; then ",
    "oxide_docker_ps=$(sudo -n docker ps -a --no-trunc --format '",
    "{{json .}}",
    "' 2>&1); ",
    "oxide_docker_status=$?; ",
    "fi; ",
    "if [ \"$oxide_docker_status\" -eq 0 ]; then ",
    "printf '%s\\n' \"$oxide_docker_ps\" | sed 's/^/PS\\t/'; ",
    "oxide_docker_ids=$(docker ps -aq --no-trunc 2>/dev/null); ",
    "oxide_docker_ids_status=$?; ",
    "if [ \"$oxide_docker_ids_status\" -ne 0 ]; then ",
    "oxide_docker_ids=$(sudo -n docker ps -aq --no-trunc 2>/dev/null); ",
    "oxide_docker_ids_status=$?; ",
    "fi; ",
    "if [ \"$oxide_docker_ids_status\" -eq 0 ] && [ -n \"$oxide_docker_ids\" ]; then ",
    "oxide_docker_inspect=$(docker inspect --format '",
    "{{json .}}",
    "' $oxide_docker_ids 2>/dev/null); ",
    "oxide_docker_inspect_status=$?; ",
    "if [ \"$oxide_docker_inspect_status\" -ne 0 ]; then ",
    "oxide_docker_inspect=$(sudo -n docker inspect --format '",
    "{{json .}}",
    "' $oxide_docker_ids 2>/dev/null); ",
    "oxide_docker_inspect_status=$?; ",
    "fi; ",
    "if [ \"$oxide_docker_inspect_status\" -eq 0 ]; then ",
    "printf '%s\\n' \"$oxide_docker_inspect\" | sed 's/^/INSPECT\\t/'; ",
    "fi; ",
    "fi; ",
    "else ",
    "printf '__OXIDE_DOCKER_ERROR__\\t%s\\n' \"$(printf '%s' \"$oxide_docker_ps\" | head -n 1 | tr '\\t' ' ')\"; ",
    "fi; ",
    "else ",
    "echo '__OXIDE_DOCKER_UNAVAILABLE__'; ",
    "fi; ",
    "echo '===DOCKER_END==='"
);

const DOCKER_SAMPLE_COMMAND_WINDOWS: &str = concat!(
    "Write-Output '===DOCKER===';",
    "if(Get-Command docker -ErrorAction SilentlyContinue){",
    "$oxideDockerPs=& docker ps -a --no-trunc --format '",
    "{{json .}}",
    "' 2>&1;",
    "if($LASTEXITCODE -eq 0){",
    "$oxideDockerPs|ForEach-Object{Write-Output ('PS'+[char]9+$_)};",
    "$oxideDockerIds=& docker ps -aq --no-trunc 2>$null;",
    "if($LASTEXITCODE -eq 0 -and $oxideDockerIds){",
    "$oxideDockerInspect=& docker inspect --format '",
    "{{json .}}",
    "' $oxideDockerIds 2>$null;",
    "if($LASTEXITCODE -eq 0){$oxideDockerInspect|ForEach-Object{Write-Output ('INSPECT'+[char]9+$_)}}",
    "}",
    "}else{",
    "Write-Output ('__OXIDE_DOCKER_ERROR__'+[char]9+($oxideDockerPs|Select-Object -First 1))",
    "}",
    "}else{Write-Output '__OXIDE_DOCKER_UNAVAILABLE__'};",
    "Write-Output '===DOCKER_END===';"
);

pub fn docker_sample_command(os_type: &str) -> &'static str {
    match os_type {
        "Windows" | "windows" => DOCKER_SAMPLE_COMMAND_WINDOWS,
        _ => DOCKER_SAMPLE_COMMAND_UNIX,
    }
}

pub fn parse_docker_snapshot(output: &str) -> ResourceDockerSnapshot {
    let Some(section) = extract_section(output, "DOCKER") else {
        return ResourceDockerSnapshot::default();
    };

    let mut containers = Vec::new();
    let mut inspect_by_id = HashMap::new();
    for line in section
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
    {
        if line == DOCKER_UNAVAILABLE_MARKER {
            return ResourceDockerSnapshot {
                status: ResourceDockerStatus::Unavailable,
                containers: Vec::new(),
            };
        }
        if let Some(message) = line.strip_prefix(DOCKER_ERROR_MARKER) {
            return ResourceDockerSnapshot {
                status: ResourceDockerStatus::Error {
                    message: clean_error_marker_message(message),
                },
                containers: Vec::new(),
            };
        }
        if let Some(inspect) = parse_docker_inspect_line(line) {
            inspect_by_id.insert(inspect.id.clone(), inspect);
            continue;
        }
        if let Some(container) = parse_docker_container_line(line) {
            containers.push(container);
        }
    }
    apply_docker_inspect_overrides(&mut containers, &inspect_by_id);

    ResourceDockerSnapshot {
        status: ResourceDockerStatus::Available,
        containers,
    }
}

pub fn visible_docker_rows(
    containers: &[ResourceDockerContainer],
    query: &str,
) -> Vec<ResourceDockerContainer> {
    let query = query.trim().to_lowercase();
    if query.is_empty() {
        return containers.to_vec();
    }
    containers
        .iter()
        .filter(|container| docker_container_matches_query(container, &query))
        .cloned()
        .collect()
}

pub fn docker_row_signature(container: &ResourceDockerContainer) -> u64 {
    let mut hasher = DefaultHasher::new();
    container.id.hash(&mut hasher);
    container.name.hash(&mut hasher);
    container.image.hash(&mut hasher);
    container.state.hash(&mut hasher);
    container.status.hash(&mut hasher);
    container.ports.hash(&mut hasher);
    hasher.finish()
}

pub fn docker_state_label_key(state: &str) -> &'static str {
    match state.trim().to_lowercase().as_str() {
        "running" => "sidebar.host_docker.states.running",
        "exited" => "sidebar.host_docker.states.exited",
        "created" => "sidebar.host_docker.states.created",
        "paused" => "sidebar.host_docker.states.paused",
        "restarting" => "sidebar.host_docker.states.restarting",
        "removing" => "sidebar.host_docker.states.removing",
        "dead" => "sidebar.host_docker.states.dead",
        _ => "sidebar.host_docker.states.unknown",
    }
}

pub fn build_docker_action_command(
    os_type: &str,
    container_id: &str,
    action: DockerActionKind,
) -> Result<DockerActionCommand, String> {
    let container_id = validated_container_id(container_id)?;
    let operation = match action {
        DockerActionKind::Start => "start",
        DockerActionKind::Stop => "stop",
        DockerActionKind::Restart => "restart",
    };
    let command = build_docker_cli_command(os_type, operation, container_id);
    Ok(DockerActionCommand { command })
}

pub fn build_docker_logs_command(
    os_type: &str,
    container_id: &str,
) -> Result<DockerCaptureCommand, String> {
    let container_id = validated_container_id(container_id)?;
    let command = build_docker_cli_command(os_type, "logs --tail 200 --timestamps", container_id);
    Ok(DockerCaptureCommand { command })
}

pub fn build_docker_exec_shell_command(container_id: &str) -> Result<String, String> {
    let container_id = validated_container_id(container_id)?;
    Ok(format!(
        "docker exec -it {container_id} sh -lc 'if command -v bash >/dev/null 2>&1; then exec bash; else exec sh; fi'"
    ))
}

pub fn build_docker_follow_logs_command(container_id: &str) -> Result<String, String> {
    let container_id = validated_container_id(container_id)?;
    Ok(format!(
        "docker logs -f --tail 200 --timestamps {container_id}"
    ))
}

fn build_docker_cli_command(os_type: &str, operation: &str, container_id: &str) -> String {
    let command = match os_type {
        "Windows" | "windows" => format!(
            "powershell -NoProfile -ExecutionPolicy Bypass -Command \"docker {operation} {container_id}\""
        ),
        _ => format!(
            "docker {operation} {container_id} 2>&1 || sudo -n docker {operation} {container_id}"
        ),
    };
    command
}

pub fn docker_action_succeeded(exit_code: Option<i32>) -> bool {
    exit_code.unwrap_or(0) == 0
}

pub fn docker_action_success_message(stdout: &str, stderr: &str) -> String {
    compact_docker_command_message(stdout)
        .or_else(|| compact_docker_command_message(stderr))
        .unwrap_or_else(|| "Docker action completed.".to_string())
}

pub fn docker_action_failure_message(stdout: &str, stderr: &str, exit_code: Option<i32>) -> String {
    compact_docker_command_message(stderr)
        .or_else(|| compact_docker_command_message(stdout))
        .unwrap_or_else(|| {
            exit_code
                .map(|code| format!("Docker action failed with exit code {code}."))
                .unwrap_or_else(|| "Docker action failed.".to_string())
        })
}

fn parse_docker_container_line(line: &str) -> Option<ResourceDockerContainer> {
    let line = line.strip_prefix("PS\t").unwrap_or(line);
    if line.starts_with("INSPECT\t") {
        return None;
    }

    if let Some(container) = parse_docker_container_json_line(line) {
        return Some(container);
    }

    let parts = line.splitn(6, '\t').collect::<Vec<_>>();
    if parts.len() < 5 {
        return None;
    }
    let id = parts[0].trim();
    if id.is_empty() {
        return None;
    }
    Some(ResourceDockerContainer {
        id: id.to_string(),
        name: clean_container_field(parts[1]).unwrap_or_else(|| id.to_string()),
        image: clean_container_field(parts[2]).unwrap_or_else(|| "-".to_string()),
        state: clean_container_field(parts[3]).unwrap_or_else(|| "unknown".to_string()),
        status: clean_container_field(parts[4]).unwrap_or_else(|| "-".to_string()),
        ports: parts.get(5).and_then(|value| clean_container_field(value)),
    })
}

fn parse_docker_inspect_line(line: &str) -> Option<DockerInspectOverride> {
    let payload = line.strip_prefix("INSPECT\t")?;
    let value = serde_json::from_str::<serde_json::Value>(payload).ok()?;
    let id = clean_json_container_field(&value, "Id")
        .or_else(|| clean_json_container_field(&value, "ID"))?;
    if id.is_empty() {
        return None;
    }

    let name = clean_json_container_field(&value, "Name").map(clean_container_name);
    let image = value
        .get("Config")
        .and_then(|config| clean_json_container_field(config, "Image"))
        .or_else(|| clean_json_container_field(&value, "Image"));

    Some(DockerInspectOverride { id, name, image })
}

fn parse_docker_container_json_line(line: &str) -> Option<ResourceDockerContainer> {
    let value = serde_json::from_str::<serde_json::Value>(line).ok()?;
    let id = clean_json_container_field(&value, "ID")
        .or_else(|| clean_json_container_field(&value, "IDFull"))?;
    if id.is_empty() {
        return None;
    }

    Some(ResourceDockerContainer {
        id: id.to_string(),
        name: clean_json_container_field(&value, "Names")
            .or_else(|| clean_json_container_field(&value, "Name"))
            .map(clean_container_name)
            .unwrap_or_else(|| id.to_string()),
        image: clean_json_container_field(&value, "Image").unwrap_or_else(|| "-".to_string()),
        state: clean_json_container_field(&value, "State").unwrap_or_else(|| "unknown".to_string()),
        status: clean_json_container_field(&value, "Status").unwrap_or_else(|| "-".to_string()),
        ports: clean_json_container_field(&value, "Ports"),
    })
}

fn apply_docker_inspect_overrides(
    containers: &mut [ResourceDockerContainer],
    inspect_by_id: &HashMap<String, DockerInspectOverride>,
) {
    for container in containers {
        let Some(inspect) = docker_inspect_for_container(container, inspect_by_id) else {
            continue;
        };
        if let Some(name) = inspect.name.as_ref() {
            container.name = name.clone();
        }
        if let Some(image) = inspect.image.as_ref() {
            container.image = image.clone();
        }
    }
}

fn docker_inspect_for_container<'a>(
    container: &ResourceDockerContainer,
    inspect_by_id: &'a HashMap<String, DockerInspectOverride>,
) -> Option<&'a DockerInspectOverride> {
    inspect_by_id.get(&container.id).or_else(|| {
        inspect_by_id
            .iter()
            .find(|(id, _)| id.starts_with(&container.id) || container.id.starts_with(id.as_str()))
            .map(|(_, inspect)| inspect)
    })
}

fn docker_container_matches_query(container: &ResourceDockerContainer, query: &str) -> bool {
    container.id.to_lowercase().contains(query)
        || container.name.to_lowercase().contains(query)
        || container.image.to_lowercase().contains(query)
        || container.state.to_lowercase().contains(query)
        || container
            .ports
            .as_deref()
            .is_some_and(|ports| ports.to_lowercase().contains(query))
}

fn validated_container_id(container_id: &str) -> Result<&str, String> {
    let container_id = container_id.trim();
    if container_id.len() < 12
        || !container_id
            .chars()
            .all(|character| character.is_ascii_hexdigit())
    {
        return Err("Invalid Docker container id.".to_string());
    }
    Ok(container_id)
}

fn clean_container_field(value: &str) -> Option<String> {
    let value = value.trim();
    if value.is_empty() || value.chars().all(|character| character == '.') {
        return None;
    }
    Some(value.to_string())
}

fn clean_json_container_field(value: &serde_json::Value, key: &str) -> Option<String> {
    value
        .get(key)
        .and_then(serde_json::Value::as_str)
        .and_then(clean_container_field)
}

fn clean_container_name(value: String) -> String {
    value.trim_start_matches('/').to_string()
}

fn clean_error_marker_message(value: &str) -> String {
    let value = value.trim_start_matches('\t').trim();
    if value.is_empty() {
        "Docker command failed.".to_string()
    } else {
        value.chars().take(180).collect()
    }
}

fn compact_docker_command_message(value: &str) -> Option<String> {
    let summary = value
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())?
        .chars()
        .take(180)
        .collect::<String>();
    Some(summary)
}

fn extract_section<'a>(output: &'a str, name: &str) -> Option<&'a str> {
    let start = format!("==={name}===");
    let end = format!("==={name}_END===");
    let after_start = output.split_once(&start)?.1;
    Some(after_start.split_once(&end)?.0.trim())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn docker_parser_reads_container_rows() {
        let output = "===DOCKER===\nabc123def456\tweb\tnginx:alpine\trunning\tUp 2 minutes\t0.0.0.0:80->80/tcp\nfff111eee222\tdb\tpostgres:16\texited\tExited (0) 1 hour ago\t\n===DOCKER_END===";

        let snapshot = parse_docker_snapshot(output);

        assert_eq!(snapshot.status, ResourceDockerStatus::Available);
        assert_eq!(snapshot.containers.len(), 2);
        assert_eq!(snapshot.containers[0].name, "web");
        assert_eq!(
            snapshot.containers[0].ports.as_deref(),
            Some("0.0.0.0:80->80/tcp")
        );
        assert_eq!(snapshot.containers[1].state, "exited");
    }

    #[test]
    fn docker_parser_reads_json_container_rows_without_truncating_names() {
        let output = concat!(
            "===DOCKER===\n",
            "PS\t{\"ID\":\"abc123def4567890\",\"Names\":\"oxideterm-cloud-sync\",\"Image\":\"ghcr.io/analyse-decircuit/oxideterm-cloud-sync:latest\",\"State\":\"running\",\"Status\":\"Up 7 weeks\",\"Ports\":\"0.0.0.0:8730->8730/tcp\"}\n",
            "PS\t{\"ID\":\"fff111eee2223334\",\"Names\":\"postgres-data\",\"Image\":\"postgres:16\",\"State\":\"exited\",\"Status\":\"Exited (0) 3 months ago\",\"Ports\":\"\"}\n",
            "===DOCKER_END===",
        );

        let snapshot = parse_docker_snapshot(output);

        assert_eq!(snapshot.status, ResourceDockerStatus::Available);
        assert_eq!(snapshot.containers.len(), 2);
        assert_eq!(snapshot.containers[0].name, "oxideterm-cloud-sync");
        assert_eq!(
            snapshot.containers[0].image,
            "ghcr.io/analyse-decircuit/oxideterm-cloud-sync:latest"
        );
        assert_eq!(
            snapshot.containers[0].ports.as_deref(),
            Some("0.0.0.0:8730->8730/tcp")
        );
        assert_eq!(snapshot.containers[1].name, "postgres-data");
        assert_eq!(snapshot.containers[1].ports, None);
    }

    #[test]
    fn docker_parser_uses_inspect_to_replace_truncated_ps_fields() {
        let output = concat!(
            "===DOCKER===\n",
            "PS\t{\"ID\":\"abc123def4567890\",\"Names\":\"...\",\"Image\":\"...\",\"State\":\"running\",\"Status\":\"Up 7 weeks\",\"Ports\":\"127.0.0.1:2375->2375/tcp\"}\n",
            "INSPECT\t{\"Id\":\"abc123def4567890\",\"Name\":\"/oxideterm-cloud-sync\",\"Config\":{\"Image\":\"ghcr.io/analyse-decircuit/oxideterm-cloud-sync:latest\"}}\n",
            "===DOCKER_END===",
        );

        let snapshot = parse_docker_snapshot(output);

        assert_eq!(snapshot.containers.len(), 1);
        assert_eq!(snapshot.containers[0].name, "oxideterm-cloud-sync");
        assert_eq!(
            snapshot.containers[0].image,
            "ghcr.io/analyse-decircuit/oxideterm-cloud-sync:latest"
        );
        assert_eq!(
            snapshot.containers[0].ports.as_deref(),
            Some("127.0.0.1:2375->2375/tcp")
        );
    }

    #[test]
    fn docker_parser_reports_unavailable_and_errors() {
        assert_eq!(
            parse_docker_snapshot("===DOCKER===\n__OXIDE_DOCKER_UNAVAILABLE__\n===DOCKER_END===")
                .status,
            ResourceDockerStatus::Unavailable
        );
        assert_eq!(
            parse_docker_snapshot(
                "===DOCKER===\n__OXIDE_DOCKER_ERROR__\tpermission denied\n===DOCKER_END==="
            )
            .status,
            ResourceDockerStatus::Error {
                message: "permission denied".to_string()
            }
        );
    }

    #[test]
    fn docker_actions_validate_container_ids() {
        assert!(
            build_docker_action_command("Linux", "abc123def456", DockerActionKind::Restart)
                .unwrap()
                .command
                .contains("docker restart abc123def456")
        );
        assert!(
            build_docker_action_command("Linux", "abc123;rm -rf /", DockerActionKind::Stop)
                .is_err()
        );
    }

    #[test]
    fn docker_logs_and_exec_commands_validate_container_ids() {
        let logs = build_docker_logs_command("Linux", "abc123def456").unwrap();
        assert!(
            logs.command
                .contains("docker logs --tail 200 --timestamps abc123def456")
        );
        assert!(
            logs.command
                .contains("sudo -n docker logs --tail 200 --timestamps abc123def456")
        );

        let exec = build_docker_exec_shell_command("abc123def456").unwrap();
        assert!(exec.contains("docker exec -it abc123def456"));
        assert!(build_docker_exec_shell_command("abc123;bad").is_err());

        let follow = build_docker_follow_logs_command("abc123def456").unwrap();
        assert_eq!(
            follow,
            "docker logs -f --tail 200 --timestamps abc123def456"
        );
        assert!(build_docker_follow_logs_command("abc123;bad").is_err());
    }

    #[test]
    fn docker_search_checks_operational_fields() {
        let snapshot = parse_docker_snapshot(
            "===DOCKER===\nabc123def456\tweb\tnginx:alpine\trunning\tUp\t80/tcp\nfff111eee222\tdb\tpostgres:16\texited\tExited\t\n===DOCKER_END===",
        );

        assert_eq!(visible_docker_rows(&snapshot.containers, "nginx").len(), 1);
        assert_eq!(visible_docker_rows(&snapshot.containers, "exited").len(), 1);
    }

    #[test]
    fn docker_sample_command_keeps_machine_readable_format() {
        let expected_format = "{{json .}}";
        assert!(docker_sample_command("Linux").contains(expected_format));
        assert!(docker_sample_command("Windows").contains(expected_format));
        assert!(docker_sample_command("Linux").contains("--no-trunc"));
        assert!(docker_sample_command("Windows").contains("--no-trunc"));
        assert!(docker_sample_command("Linux").contains("docker inspect --format"));
        assert!(docker_sample_command("Windows").contains("docker inspect --format"));
    }
}
