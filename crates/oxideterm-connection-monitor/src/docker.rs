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
    Incomplete,
    OutputLimit,
    DaemonUnavailable {
        message: String,
    },
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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DockerActionAvailability {
    pub can_start: bool,
    pub can_stop: bool,
    pub can_restart: bool,
    pub can_use_live_tools: bool,
}

/// Returns the actions supported by the container's current lifecycle state.
pub fn docker_action_availability(state: &str) -> DockerActionAvailability {
    let is_live = matches!(
        state.trim().to_ascii_lowercase().as_str(),
        "running" | "restarting" | "paused"
    );
    DockerActionAvailability {
        can_start: !is_live,
        can_stop: is_live,
        can_restart: is_live,
        can_use_live_tools: is_live,
    }
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
    state: Option<String>,
}

const DOCKER_UNAVAILABLE_MARKER: &str = "__OXIDE_DOCKER_UNAVAILABLE__";
const DOCKER_ERROR_MARKER: &str = "__OXIDE_DOCKER_ERROR__";

// Generate JSON keys inside the template so legacy PowerShell native argument passing
// cannot strip their quotes. Only fields consumed by this panel cross the SSH channel.
// Docker CLI before 20.10 has no .State in ps templates; inspect supplies the exact state.
macro_rules! docker_ps_format {
    () => { r#"{ {{json `ID`}}:{{json .ID}},{{json `Names`}}:{{json .Names}},{{json `Image`}}:{{json .Image}},{{json `Status`}}:{{json .Status}},{{json `Ports`}}:{{json .Ports}}}"# };
}

macro_rules! docker_inspect_format {
    () => {
        r#"{ {{json `Id`}}:{{json .Id}},{{json `Name`}}:{{json .Name}},{{json `Config`}}:{ {{json `Image`}}:{{json .Config.Image}}},{{json `State`}}:{ {{json `Status`}}:{{json .State.Status}}}}"#
    };
}

// Keep the user's Docker context and original failure unless a permission retry succeeds.
macro_rules! docker_unix_runner {
    () => {
        concat!(
            "oxide_docker() { ",
            "oxide_docker_output=$(docker \"$@\" 2>&1); oxide_docker_result=$?; ",
            "if [ \"$oxide_docker_result\" -ne 0 ]; then ",
            "case \"$oxide_docker_output\" in *'permission denied'*|*'Permission denied'*) ",
            "oxide_docker_sudo_output=$(sudo -n docker \"$@\" 2>&1); ",
            "if [ $? -eq 0 ]; then oxide_docker_output=$oxide_docker_sudo_output; oxide_docker_result=0; fi; ",
            ";; esac; fi; ",
            "printf '%s\\n' \"$oxide_docker_output\"; return \"$oxide_docker_result\"; }; "
        )
    };
}

const DOCKER_SAMPLE_COMMAND_UNIX: &str = concat!(
    docker_unix_runner!(),
    "echo '===DOCKER==='; ",
    "if command -v docker >/dev/null 2>&1; then ",
    "oxide_docker_ps=$(oxide_docker ps -a --no-trunc --format '",
    docker_ps_format!(),
    "' 2>&1); ",
    "oxide_docker_status=$?; ",
    "if [ \"$oxide_docker_status\" -eq 0 ]; then ",
    "printf '%s\\n' \"$oxide_docker_ps\" | sed 's/^/PS\\t/'; ",
    "oxide_docker_ids=$(oxide_docker ps -aq --no-trunc 2>/dev/null); ",
    "oxide_docker_ids_status=$?; ",
    "if [ \"$oxide_docker_ids_status\" -eq 0 ] && [ -n \"$oxide_docker_ids\" ]; then ",
    "oxide_docker_inspect=$(oxide_docker inspect --format '",
    docker_inspect_format!(),
    "' $oxide_docker_ids 2>/dev/null); ",
    "oxide_docker_inspect_status=$?; ",
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
    docker_ps_format!(),
    "' 2>&1;",
    "if($LASTEXITCODE -eq 0){",
    "$oxideDockerPs|ForEach-Object{Write-Output ('PS'+[char]9+$_)};",
    "$oxideDockerIds=& docker ps -aq --no-trunc 2>$null;",
    "if($LASTEXITCODE -eq 0 -and $oxideDockerIds){",
    "$oxideDockerInspect=& docker inspect --format '",
    docker_inspect_format!(),
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
        return ResourceDockerSnapshot {
            status: if output.lines().any(|line| line.trim() == "===DOCKER===") {
                ResourceDockerStatus::Incomplete
            } else {
                ResourceDockerStatus::Unknown
            },
            containers: Vec::new(),
        };
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
            let lower = message.to_ascii_lowercase();
            let message = clean_error_marker_message(message);
            let daemon_unavailable = !lower.contains("permission denied")
                && (lower.contains("cannot connect to the docker daemon")
                    || lower.contains("is the docker daemon running")
                    || (lower.contains("failed to connect")
                        && lower.contains("docker")
                        && (lower.contains("connection refused")
                            || lower.contains("no such file or directory")))
                    || (lower.contains("open //./pipe/")
                        && lower.contains("the system cannot find the file specified")));
            return ResourceDockerSnapshot {
                status: if daemon_unavailable {
                    ResourceDockerStatus::DaemonUnavailable { message }
                } else {
                    ResourceDockerStatus::Error { message }
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

/// Produces a stable identity signature without treating sampled status as row geometry.
pub fn docker_row_signature(container: &ResourceDockerContainer) -> u64 {
    let mut hasher = DefaultHasher::new();
    container.id.hash(&mut hasher);
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
        "docker exec -it {container_id} sh -lc \"if command -v bash >/dev/null 2>&1; then exec bash; else exec sh; fi\""
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
            "{}oxide_docker {operation} {container_id}",
            docker_unix_runner!()
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

    let state = value
        .get("State")
        .and_then(|state| clean_json_container_field(state, "Status"));

    Some(DockerInspectOverride {
        id,
        name,
        image,
        state,
    })
}

fn parse_docker_container_json_line(line: &str) -> Option<ResourceDockerContainer> {
    let value = serde_json::from_str::<serde_json::Value>(line).ok()?;
    let id = clean_json_container_field(&value, "ID")
        .or_else(|| clean_json_container_field(&value, "IDFull"))?;
    if id.is_empty() {
        return None;
    }

    let status = clean_json_container_field(&value, "Status").unwrap_or_else(|| "-".to_string());
    let state = clean_json_container_field(&value, "State")
        .unwrap_or_else(|| docker_state_from_status(&status).to_string());
    Some(ResourceDockerContainer {
        id: id.to_string(),
        name: clean_json_container_field(&value, "Names")
            .or_else(|| clean_json_container_field(&value, "Name"))
            .map(clean_container_name)
            .unwrap_or_else(|| id.to_string()),
        image: clean_json_container_field(&value, "Image").unwrap_or_else(|| "-".to_string()),
        state,
        status,
        ports: clean_json_container_field(&value, "Ports"),
    })
}

fn docker_state_from_status(status: &str) -> &'static str {
    // Inspect can fail independently of ps, for example if a container disappears mid-sample.
    match status.split_whitespace().next() {
        Some("Up") if status.ends_with("(Paused)") => "paused",
        Some("Up") => "running",
        Some("Restarting") => "restarting",
        Some("Exited") => "exited",
        Some("Created") => "created",
        Some("Dead") => "dead",
        _ if status == "Removal In Progress" => "removing",
        _ => "unknown",
    }
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
        if let Some(state) = inspect.state.as_ref() {
            container.state = state.clone();
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
    // Match whole lines: PTY command echo and JSON string values may contain marker text.
    let mut section_start = None;
    let mut offset = 0;
    for line in output.split_inclusive('\n') {
        if line.trim() == start {
            section_start = Some(offset + line.len());
        } else if line.trim() == end {
            return section_start.map(|start| output[start..offset].trim());
        }
        offset += line.len();
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(unix)]
    #[test]
    fn docker_commands_preserve_original_failure_when_sudo_cannot_help() {
        for error in [
            "Cannot connect to the Docker daemon at unix:///var/run/docker.sock. Is the docker daemon running?",
            "permission denied while trying to connect to the Docker daemon socket",
        ] {
            let fixture = format!(
                "docker() {{ printf '%s\\n' '{error}' >&2; return 17; }}; sudo() {{ printf '%s\\n' 'sudo: a password is required' >&2; return 1; }};"
            );
            for (kind, command) in [
                ("sample", docker_sample_command("Linux").to_string()),
                (
                    "action",
                    build_docker_action_command("Linux", "abc123def456", DockerActionKind::Restart)
                        .unwrap()
                        .command,
                ),
                (
                    "logs",
                    build_docker_logs_command("Linux", "abc123def456")
                        .unwrap()
                        .command,
                ),
            ] {
                let output = std::process::Command::new("sh")
                    .arg("-c")
                    .arg(format!("{fixture}\n{command}"))
                    .output()
                    .unwrap();
                let stdout = String::from_utf8(output.stdout).unwrap();
                let stderr = String::from_utf8(output.stderr).unwrap();
                assert!(stdout.contains(error), "{kind}: {stdout}");
                assert!(!stdout.contains("sudo:"), "{kind}: {stdout}");
                assert_eq!(stderr, "", "{kind}");
                assert_eq!(
                    output.status.code(),
                    Some(if kind == "sample" { 0 } else { 17 }),
                    "{kind}"
                );
            }
        }
    }

    #[cfg(unix)]
    #[test]
    fn docker_commands_retry_only_permission_errors() {
        for (error, expected_sudo) in [
            (
                "Cannot connect to the Docker daemon. Is the docker daemon running?",
                false,
            ),
            ("Error response from daemon: container is restarting", false),
            (
                "permission denied while trying to connect to the Docker daemon socket",
                true,
            ),
        ] {
            let fixture = format!(
                "docker() {{ printf '%s\\n' '{error}' >&2; return 17; }}; sudo() {{ printf '%s\\n' 'sudo succeeded'; }};"
            );
            for command in [
                docker_sample_command("Linux").to_string(),
                build_docker_action_command("Linux", "abc123def456", DockerActionKind::Restart)
                    .unwrap()
                    .command,
                build_docker_logs_command("Linux", "abc123def456")
                    .unwrap()
                    .command,
            ] {
                let output = std::process::Command::new("sh")
                    .args(["-c", &format!("{fixture}\n{command}")])
                    .output()
                    .unwrap();
                let stdout = String::from_utf8(output.stdout).unwrap();
                assert_eq!(
                    stdout.contains("sudo succeeded"),
                    expected_sudo,
                    "{error}: {stdout}"
                );
                assert_eq!(stdout.contains(error), !expected_sudo, "{error}: {stdout}");
            }
        }
    }

    #[test]
    fn docker_parser_preserves_container_fields_and_applies_inspect_overrides() {
        for (case, output, expected) in [
            (
                "tab rows",
                "===DOCKER===\nabc123def456\tweb\tnginx:alpine\trunning\tUp 2 minutes\t0.0.0.0:80->80/tcp\nfff111eee222\tdb\tpostgres:16\texited\tExited (0) 1 hour ago\t\n===DOCKER_END===",
                vec![
                    (
                        "abc123def456",
                        "web",
                        "nginx:alpine",
                        "running",
                        "Up 2 minutes",
                        Some("0.0.0.0:80->80/tcp"),
                    ),
                    (
                        "fff111eee222",
                        "db",
                        "postgres:16",
                        "exited",
                        "Exited (0) 1 hour ago",
                        None,
                    ),
                ],
            ),
            (
                "JSON rows",
                concat!(
                    "===DOCKER===\n",
                    "PS\t{\"ID\":\"abc123def4567890\",\"Names\":\"oxideterm-cloud-sync\",\"Image\":\"ghcr.io/analyse-decircuit/oxideterm-cloud-sync:latest\",\"State\":\"running\",\"Status\":\"Up 7 weeks\",\"Ports\":\"0.0.0.0:8730->8730/tcp\"}\n",
                    "PS\t{\"ID\":\"fff111eee2223334\",\"Names\":\"postgres-data\",\"Image\":\"postgres:16\",\"State\":\"exited\",\"Status\":\"Exited (0) 3 months ago\",\"Ports\":\"\"}\n",
                    "===DOCKER_END===",
                ),
                vec![
                    (
                        "abc123def4567890",
                        "oxideterm-cloud-sync",
                        "ghcr.io/analyse-decircuit/oxideterm-cloud-sync:latest",
                        "running",
                        "Up 7 weeks",
                        Some("0.0.0.0:8730->8730/tcp"),
                    ),
                    (
                        "fff111eee2223334",
                        "postgres-data",
                        "postgres:16",
                        "exited",
                        "Exited (0) 3 months ago",
                        None,
                    ),
                ],
            ),
            (
                "inspect overrides",
                concat!(
                    "===DOCKER===\n",
                    "PS\t{\"ID\":\"abc123def4567890\",\"Names\":\"...\",\"Image\":\"...\",\"State\":\"running\",\"Status\":\"Up 7 weeks\",\"Ports\":\"127.0.0.1:2375->2375/tcp\"}\n",
                    "INSPECT\t{\"Id\":\"abc123def4567890\",\"Name\":\"/oxideterm-cloud-sync\",\"Config\":{\"Image\":\"ghcr.io/analyse-decircuit/oxideterm-cloud-sync:latest\"},\"State\":{\"Status\":\"paused\"}}\n",
                    "===DOCKER_END===",
                ),
                vec![(
                    "abc123def4567890",
                    "oxideterm-cloud-sync",
                    "ghcr.io/analyse-decircuit/oxideterm-cloud-sync:latest",
                    "paused",
                    "Up 7 weeks",
                    Some("127.0.0.1:2375->2375/tcp"),
                )],
            ),
        ] {
            let snapshot = parse_docker_snapshot(output);
            assert_eq!(snapshot.status, ResourceDockerStatus::Available, "{case}");
            assert_eq!(
                snapshot
                    .containers
                    .iter()
                    .map(|container| (
                        container.id.as_str(),
                        container.name.as_str(),
                        container.image.as_str(),
                        container.state.as_str(),
                        container.status.as_str(),
                        container.ports.as_deref(),
                    ))
                    .collect::<Vec<_>>(),
                expected,
                "{case}"
            );
        }
    }

    #[test]
    fn docker_status_text_preserves_actions_when_inspect_is_unavailable() {
        for (status, state, live, label) in [
            (
                "Up 2 hours",
                "running",
                true,
                "sidebar.host_docker.states.running",
            ),
            (
                "Up 2 hours (healthy)",
                "running",
                true,
                "sidebar.host_docker.states.running",
            ),
            (
                "Up 2 hours (unhealthy)",
                "running",
                true,
                "sidebar.host_docker.states.running",
            ),
            (
                "Up 2 hours (Paused)",
                "paused",
                true,
                "sidebar.host_docker.states.paused",
            ),
            (
                "Restarting (1) 5 seconds ago",
                "restarting",
                true,
                "sidebar.host_docker.states.restarting",
            ),
            (
                "Exited (0) 3 days ago",
                "exited",
                false,
                "sidebar.host_docker.states.exited",
            ),
            (
                "Created",
                "created",
                false,
                "sidebar.host_docker.states.created",
            ),
            (
                "Removal In Progress",
                "removing",
                false,
                "sidebar.host_docker.states.removing",
            ),
            ("Dead", "dead", false, "sidebar.host_docker.states.dead"),
            ("", "unknown", false, "sidebar.host_docker.states.unknown"),
            (
                "Unexpected status",
                "unknown",
                false,
                "sidebar.host_docker.states.unknown",
            ),
        ] {
            let output = format!(
                "===DOCKER===\nPS\t{{\"ID\":\"abc123def456\",\"Names\":\"web\",\"Image\":\"nginx:alpine\",\"Status\":{}}}\n===DOCKER_END===",
                serde_json::to_string(status).unwrap(),
            );
            let snapshot = parse_docker_snapshot(&output);
            assert_eq!(
                snapshot.containers,
                vec![ResourceDockerContainer {
                    id: "abc123def456".into(),
                    name: "web".into(),
                    image: "nginx:alpine".into(),
                    state: state.into(),
                    status: if status.is_empty() { "-" } else { status }.into(),
                    ports: None,
                }],
                "{status}"
            );
            assert_eq!(
                docker_action_availability(&snapshot.containers[0].state),
                DockerActionAvailability {
                    can_start: !live,
                    can_stop: live,
                    can_restart: live,
                    can_use_live_tools: live,
                },
                "{status}"
            );
            assert_eq!(
                docker_state_label_key(&snapshot.containers[0].state),
                label,
                "{status}"
            );
        }
    }

    #[test]
    fn docker_parser_reports_unavailable_and_errors() {
        for output in [
            "===DOCKER===\nPS\t{\"ID\":\"abc123def456\",\"Names\":\"web\"}\n",
            "===DOCKER===\nPS\t{\"ID\":\"abc123def456\",\"Names\":\"===DOCKER_END===\"}\n",
        ] {
            let snapshot = parse_docker_snapshot(output);
            assert_eq!(snapshot.status, ResourceDockerStatus::Incomplete);
            assert!(snapshot.containers.is_empty());
        }
        assert_eq!(
            parse_docker_snapshot("echo '===DOCKER==='; echo '===DOCKER_END==='\n").status,
            ResourceDockerStatus::Unknown
        );
        assert_eq!(
            parse_docker_snapshot("===DOCKER===\n===DOCKER_END===\n").status,
            ResourceDockerStatus::Available
        );
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
        for (error, unavailable) in [
            (
                "Cannot connect to the Docker daemon at unix:///var/run/docker.sock. Is the docker daemon running?",
                true,
            ),
            (
                "failed to connect to the docker API at unix:///var/run/docker.sock; check if the path is correct and if the daemon is running: dial unix /var/run/docker.sock: connect: no such file or directory",
                true,
            ),
            (
                "failed to connect to the docker API at unix:///run/docker.sock: dial unix /run/docker.sock: connect: connection refused",
                true,
            ),
            (
                "error during connect: Get http://example: open //./pipe/dockerDesktopLinuxEngine: The system cannot find the file specified.",
                true,
            ),
            (
                "permission denied while trying to connect to the Docker daemon socket",
                false,
            ),
            (
                "Cannot connect to the Docker daemon: permission denied",
                false,
            ),
            (
                "Error response from daemon: client version is too old",
                false,
            ),
        ] {
            let output = format!("===DOCKER===\n__OXIDE_DOCKER_ERROR__\t{error}\n===DOCKER_END===");
            let expected_message = error.chars().take(180).collect();
            assert_eq!(
                parse_docker_snapshot(&output).status,
                if unavailable {
                    ResourceDockerStatus::DaemonUnavailable {
                        message: expected_message,
                    }
                } else {
                    ResourceDockerStatus::Error {
                        message: expected_message,
                    }
                },
                "{error}"
            );
        }
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

        for (query, expected) in [
            ("nginx", vec!["web"]),
            ("exited", vec!["db"]),
            ("missing", vec![]),
            ("", vec!["web", "db"]),
        ] {
            let rows = visible_docker_rows(&snapshot.containers, query);
            assert_eq!(
                rows.iter().map(|row| row.name.as_str()).collect::<Vec<_>>(),
                expected,
                "{query}"
            );
        }
    }

    #[test]
    fn docker_sample_command_keeps_machine_readable_format() {
        for os in ["Linux", "Windows"] {
            let command = docker_sample_command(os);
            assert!(
                !command.contains("{{json .}}"),
                "{os}: do not collect full configs or labels"
            );
            assert!(command.contains(r#"docker ps -a --no-trunc --format '{ {{json `ID`}}:{{json .ID}},{{json `Names`}}:{{json .Names}},{{json `Image`}}:{{json .Image}},{{json `Status`}}:{{json .Status}},{{json `Ports`}}:{{json .Ports}}}'"#));
            assert!(command.contains(r#"docker inspect --format '{ {{json `Id`}}:{{json .Id}},{{json `Name`}}:{{json .Name}},{{json `Config`}}:{ {{json `Image`}}:{{json .Config.Image}}},{{json `State`}}:{ {{json `Status`}}:{{json .State.Status}}}}'"#));
        }
    }
}
