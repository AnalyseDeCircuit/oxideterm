// Copyright (C) 2026 jaime.sudo
// SPDX-License-Identifier: GPL-3.0-only

//! One-shot container resource usage.
//!
//! Docker prints JSON per line, Podman prints one JSON array, and `wslc stats`
//! is already a snapshot and rejects `--no-stream`. All three are parsed by the
//! same tolerant record reader.

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::container::engine::ContainerEngine;
use crate::container::instance::ContainerInstance;

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ContainerStat {
    pub id: String,
    pub name: String,
    pub cpu_percent: String,
    pub mem_usage: String,
    pub mem_percent: String,
    pub net_io: String,
    pub block_io: String,
    pub pids: String,
    /// Parsed CPU share of one core, so the panel can sort instead of comparing
    /// formatted columns. `0` when the engine reported no value.
    #[serde(default)]
    pub cpu_percent_value: f64,
    /// Parsed memory usage in bytes, the sort key for the memory column.
    #[serde(default)]
    pub mem_usage_bytes: u64,
}

pub fn stats_command(os_type: &str, instance: &ContainerInstance) -> String {
    let program = instance.engine.program();
    let prefix = instance.cli_prefix();
    let no_stream = if instance.engine.supports_stats_no_stream() {
        "--no-stream "
    } else {
        ""
    };
    // Docker's `stats` context has no JSON projection before CLI 23.0 (the
    // `json` keyword does not exist and `{{json .}}` yields `{}`), so Docker
    // prints explicit fields; Podman and wslc keep their JSON shapes.
    let format = match instance.engine {
        ContainerEngine::Docker => {
            "--format '{{.Container}}\\t{{.Name}}\\t{{.CPUPerc}}\\t{{.MemUsage}}\\t{{.MemPerc}}\\t{{.NetIO}}\\t{{.BlockIO}}\\t{{.PIDs}}'"
        }
        _ => "--format json",
    };
    let posix = format!(
        "echo '===CONTAINER_STATS==='; \
         if command -v {program} >/dev/null 2>&1; then \
         oxide_stats=$({prefix} stats {no_stream}{format} 2>&1); oxide_rc=$?; \
         if [ \"$oxide_rc\" -eq 0 ]; then \
         printf '%s\\n' \"$oxide_stats\" | awk '{{print \"STAT\\t\" $0}}'; \
         else printf 'ERR\\t%s\\n' \"$(printf '%s' \"$oxide_stats\" | head -n 1 | tr '\\t' ' ')\"; fi; \
         else printf 'ERR\\t%s\\n' '{program} CLI not found'; fi; \
         echo '===CONTAINER_STATS_END==='"
    );
    match os_type {
        "Windows" | "windows" => crate::container::windows::posix_via_bash(
            &posix,
            "CONTAINER_STATS",
            "Git Bash (bash.exe) not found",
        ),
        _ => posix,
    }
}

pub fn parse_stats(output: &str) -> Result<Vec<ContainerStat>, String> {
    let Some(section) = extract_section(output, "CONTAINER_STATS") else {
        return Err("Container stats output could not be read.".to_string());
    };
    if let Some(detail) = section
        .lines()
        .find_map(|line| line.strip_prefix("ERR\t"))
        .map(str::trim)
        .filter(|detail| !detail.is_empty())
    {
        return Err(detail.chars().take(200).collect());
    }
    let mut records = Vec::new();
    let payloads = section
        .lines()
        .filter_map(|line| line.strip_prefix("STAT\t"))
        .collect::<Vec<_>>();
    // Docker's fixed-field template (CLI < 23) is tab separated; the JSON
    // shapes always open with `{` or `[`. Engine warnings share the stream, so
    // the first line that looks like data decides, not the first line overall.
    let Some(start) = payloads.iter().position(|payload| {
        let payload = payload.trim_start();
        payload.starts_with('{') || payload.starts_with('[')
    }) else {
        // Docker's fixed-field template: eight tab-separated columns in the
        // order the command requests them.
        for payload in &payloads {
            if let Some(stat) = parse_table_row(payload) {
                records.push(stat);
            }
        }
        if records.is_empty() && !payloads.is_empty() {
            return Err("Container stats output could not be parsed.".to_string());
        }
        return Ok(records);
    };
    // Podman prints one array; Docker prints one object per line. Warnings
    // after the payload are ignored by both shapes: the array is cut at its
    // closing bracket and the per-line form only keeps object lines.
    let body = if payloads[start].trim_start().starts_with('[') {
        let joined = payloads[start..].join("\n");
        let Some(end) = json_value_end(&joined) else {
            return Err("Container stats output could not be parsed.".to_string());
        };
        joined[..=end].to_string()
    } else {
        format!(
            "[{}]",
            payloads[start..]
                .iter()
                .filter(|payload| payload.trim_start().starts_with('{'))
                .copied()
                .collect::<Vec<_>>()
                .join(",")
        )
    };
    let Ok(Value::Array(entries)) = serde_json::from_str::<Value>(&body) else {
        return Err("Container stats output could not be parsed.".to_string());
    };
    for entry in entries {
        // `[null]` or a scalar entry is a broken capture, not a row to drop.
        let Value::Object(object) = entry else {
            return Err("Container stats output could not be parsed.".to_string());
        };
        records.push(parse_stat(&object));
    }
    Ok(records)
}

/// Index of the bracket that closes the first JSON array/object in `text`,
/// honoring strings and escapes so a warning line after the payload cannot
/// break the parse.
fn json_value_end(text: &str) -> Option<usize> {
    let start = text.find(|character: char| character == '[' || character == '{')?;
    let mut depth = 0usize;
    let mut in_string = false;
    let mut escaped = false;
    for (index, byte) in text.as_bytes().iter().enumerate().skip(start) {
        if in_string {
            if escaped {
                escaped = false;
            } else if *byte == b'\\' {
                escaped = true;
            } else if *byte == b'"' {
                in_string = false;
            }
            continue;
        }
        match *byte {
            b'"' => in_string = true,
            b'[' | b'{' => depth += 1,
            b']' | b'}' => {
                depth = depth.saturating_sub(1);
                if depth == 0 {
                    return Some(index);
                }
            }
            _ => {}
        }
    }
    None
}

fn parse_table_row(row: &str) -> Option<ContainerStat> {
    let columns: Vec<&str> = row.split('\t').collect();
    if columns.len() < 8 {
        return None;
    }
    let mut stat = ContainerStat {
        id: columns[0].trim().to_string(),
        name: columns[1].trim().to_string(),
        cpu_percent: columns[2].trim().to_string(),
        mem_usage: columns[3].trim().to_string(),
        mem_percent: columns[4].trim().to_string(),
        net_io: columns[5].trim().to_string(),
        block_io: columns[6].trim().to_string(),
        pids: columns[7].trim().to_string(),
        cpu_percent_value: 0.0,
        mem_usage_bytes: 0,
    };
    fill_sort_values(&mut stat);
    Some(stat)
}

/// Percentages and memory sizes arrive as display text (`1.25%`,
/// `12.3MiB / 1GiB`); the table sorts on the numeric projection.
fn fill_sort_values(stat: &mut ContainerStat) {
    stat.cpu_percent_value = stat
        .cpu_percent
        .trim()
        .trim_end_matches('%')
        .trim()
        .parse::<f64>()
        .unwrap_or(0.0)
        .max(0.0);
    stat.mem_usage_bytes = stat
        .mem_usage
        .split('/')
        .next()
        .and_then(crate::container::parse::parse_size_bytes)
        .unwrap_or(0);
}

fn parse_stat(object: &Map<String, Value>) -> ContainerStat {
    let field = |names: &[&str]| -> String {
        names
            .iter()
            .find_map(|name| {
                object
                    .iter()
                    .find(|(key, _)| key.eq_ignore_ascii_case(name))
                    .and_then(|(_, value)| match value {
                        Value::String(text) => Some(text.clone()),
                        Value::Number(number) => Some(number.to_string()),
                        _ => None,
                    })
            })
            .unwrap_or_default()
    };
    let mut stat = ContainerStat {
        id: field(&["ID", "Id", "id", "Container"]),
        name: field(&["Name", "name"]),
        cpu_percent: field(&["CPUPerc", "CPUPerc", "cpu_percent", "CPU"]),
        mem_usage: field(&["MemUsage", "MemUsage", "mem_usage", "MEM USAGE / LIMIT"]),
        mem_percent: field(&["MemPerc", "mem_percent", "MEM %"]),
        net_io: field(&["NetIO", "net_io", "NET I/O"]),
        block_io: field(&["BlockIO", "block_io", "blocki", "BLOCK I/O"]),
        pids: field(&["PIDs", "PIDs", "pids"]),
        cpu_percent_value: 0.0,
        mem_usage_bytes: 0,
    };
    fill_sort_values(&mut stat);
    stat
}

fn extract_section<'a>(output: &'a str, name: &str) -> Option<&'a str> {
    let start = format!("==={name}===");
    let end = format!("==={name}_END===");
    let after_start = output.split_once(&start)?.1;
    // Truncated captures keep whatever rows arrived; only the tail is lost.
    Some(match after_start.split_once(&end) {
        Some((section, _)) => section.trim(),
        None => after_start.trim(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::container::engine::ContainerEngine;
    use crate::container::transport::ContainerTransport;

    #[test]
    fn parses_podman_array_stats() {
        let output = concat!(
            "===CONTAINER_STATS===\n",
            "STAT\t[\n",
            "STAT\t {\n",
            "STAT\t  \"id\": \"e04c3e01c195\",\n",
            "STAT\t  \"name\": \"slapd-proxy\",\n",
            "STAT\t  \"cpu_percent\": \"0.00%\",\n",
            "STAT\t  \"mem_usage\": \"47.57MB / 16.48GB\",\n",
            "STAT\t  \"mem_percent\": \"0.29%\",\n",
            "STAT\t  \"net_io\": \"21.2kB / 7.907kB\",\n",
            "STAT\t  \"block_io\": \"0B / 0B\",\n",
            "STAT\t  \"pids\": \"2\"\n",
            "STAT\t }\n",
            "STAT\t]\n",
            "===CONTAINER_STATS_END===",
        );
        let stats = parse_stats(output).expect("stats parse");
        assert_eq!(stats.len(), 1);
        assert_eq!(stats[0].name, "slapd-proxy");
        assert_eq!(stats[0].mem_usage, "47.57MB / 16.48GB");
        assert_eq!(stats[0].pids, "2");
    }

    #[test]
    fn parses_docker_line_stats() {
        let output = concat!(
            "===CONTAINER_STATS===\n",
            "STAT\t{\"BlockIO\":\"0B / 0B\",\"CPUPerc\":\"1.25%\",\"Container\":\"a1b2\",\"ID\":\"a1b2\",\"MemPerc\":\"0.5%\",\"MemUsage\":\"10MiB / 1GiB\",\"Name\":\"web\",\"NetIO\":\"1kB / 2kB\",\"PIDs\":\"3\"}\n",
            "===CONTAINER_STATS_END===",
        );
        let stats = parse_stats(output).expect("stats parse");
        assert_eq!(stats.len(), 1);
        assert_eq!(stats[0].name, "web");
        assert_eq!(stats[0].cpu_percent, "1.25%");
        assert_eq!(stats[0].pids, "3");
    }

    #[test]
    fn parses_docker_field_rows_from_cli_before_json_keyword() {
        let output = concat!(
            "===CONTAINER_STATS===\n",
            "STAT\tabc123def456\tweb\t1.25%\t10MiB / 1GiB\t0.5%\t1kB / 2kB\t0B / 0B\t3\n",
            "===CONTAINER_STATS_END===",
        );
        let stats = parse_stats(output).expect("stats parse");
        assert_eq!(stats.len(), 1);
        assert_eq!(stats[0].name, "web");
        assert_eq!(stats[0].cpu_percent, "1.25%");
        assert_eq!(stats[0].net_io, "1kB / 2kB");
        assert_eq!(stats[0].pids, "3");
    }

    #[test]
    fn leading_warnings_do_not_hide_a_podman_array() {
        // `stats` merges stderr into its output; a warning line before `[` used
        // to send the whole payload down the tabular path and drop every row.
        let output = concat!(
            "===CONTAINER_STATS===\n",
            "STAT\ttime=\"2026-11-05T00:00:00Z\" level=warning msg=\"rootless mode\"\n",
            "STAT\t[\n",
            "STAT\t {\n",
            "STAT\t  \"id\": \"e04c3e01c195\",\n",
            "STAT\t  \"name\": \"slapd-proxy\",\n",
            "STAT\t  \"cpu_percent\": \"0.00%\",\n",
            "STAT\t  \"mem_usage\": \"47.57MB / 16.48GB\",\n",
            "STAT\t  \"mem_percent\": \"0.29%\",\n",
            "STAT\t  \"net_io\": \"21.2kB / 7.907kB\",\n",
            "STAT\t  \"blocki\": \"0B / 12.3MB\",\n",
            "STAT\t  \"pids\": \"2\"\n",
            "STAT\t }\n",
            "STAT\t]\n",
            "STAT\ttime=\"2026-11-05T00:00:01Z\" level=warning msg=\"trailing\"\n",
            "===CONTAINER_STATS_END===",
        );
        let stats = parse_stats(output).expect("stats parse");
        assert_eq!(stats.len(), 1);
        assert_eq!(stats[0].name, "slapd-proxy");
        // Podman spells the key `blocki`; it used to be dropped.
        assert_eq!(stats[0].block_io, "0B / 12.3MB");
    }

    #[test]
    fn docker_objects_with_leading_and_trailing_warnings_still_parse() {
        let output = concat!(
            "===CONTAINER_STATS===\n",
            "STAT\ttime=\"2026-11-05T00:00:00Z\" level=warning msg=\"cgroup v1\"\n",
            "STAT\t{\"BlockIO\":\"0B / 0B\",\"CPUPerc\":\"1.25%\",\"Container\":\"a1b2\",\"Name\":\"web\"}\n",
            "STAT\ttime=\"2026-11-05T00:00:01Z\" level=warning msg=\"trailing\"\n",
            "===CONTAINER_STATS_END===",
        );
        let stats = parse_stats(output).expect("stats parse");
        assert_eq!(stats.len(), 1);
        assert_eq!(stats[0].name, "web");
    }

    #[test]
    fn stats_array_entries_must_be_objects() {
        for body in ["[null]", "[5]", "[{\"Name\":\"web\"},null]"] {
            let output = format!("===CONTAINER_STATS===\nSTAT\t{body}\n===CONTAINER_STATS_END===");
            assert_eq!(
                parse_stats(&output).unwrap_err(),
                "Container stats output could not be parsed."
            );
        }
    }

    #[test]
    fn stats_failure_is_reported_instead_of_empty_rows() {
        let output = "===CONTAINER_STATS===\nERR\tCannot connect to the Docker daemon\n===CONTAINER_STATS_END===";
        assert_eq!(
            parse_stats(output).unwrap_err(),
            "Cannot connect to the Docker daemon"
        );
        // A missing section is a failed capture, not an empty inventory.
        assert!(parse_stats("").is_err());
    }

    #[test]
    fn docker_stats_requests_explicit_fields_for_old_cli() {
        let docker = ContainerInstance::new(ContainerEngine::Docker, ContainerTransport::Cli, None);
        let command = stats_command("Linux", &docker);
        assert!(command.contains("{{.CPUPerc}}"));
        assert!(!command.contains("--format json"));
        assert!(command.contains("ERR\\t%s"));
    }

    #[test]
    fn stats_command_omits_no_stream_for_wslc() {
        let wslc = ContainerInstance::new(ContainerEngine::Wslc, ContainerTransport::Cli, None);
        let windows = stats_command("Windows", &wslc);
        assert!(windows.contains("FromBase64String("));
        assert!(!stats_command("Windows", &wslc).contains("--no-stream"));
        let podman = ContainerInstance::new(ContainerEngine::Podman, ContainerTransport::Cli, None);
        assert!(stats_command("Linux", &podman).contains("stats --no-stream --format json"));
    }

    #[test]
    fn sort_values_come_from_the_formatted_columns() {
        let docker = concat!(
            "===CONTAINER_STATS===\n",
            "STAT\taaa\tweb\t0.50%\t1.5GiB / 2GiB\t1.00%\t1kB / 2kB\t0B / 0B\t7\n",
            "===CONTAINER_STATS_END===",
        );
        let stats = parse_stats(docker).expect("stats");
        assert_eq!(stats[0].cpu_percent_value, 0.5);
        assert_eq!(stats[0].mem_usage_bytes, 1_610_612_736);

        let podman = concat!(
            "===CONTAINER_STATS===\n",
            "STAT\t[{\"id\":\"bbb\",\"name\":\"api\",\"cpu_percent\":\"1.25%\",\"mem_usage\":\"12.5MiB / 1GiB\"}]\n",
            "===CONTAINER_STATS_END===",
        );
        let stats = parse_stats(podman).expect("stats");
        assert_eq!(stats[0].cpu_percent_value, 1.25);
        assert_eq!(stats[0].mem_usage_bytes, 13_107_200);
    }
}
