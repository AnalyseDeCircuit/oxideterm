// Copyright (C) 2026 jaime.su
// SPDX-License-Identifier: GPL-3.0-only

//! Snapshot parsing for every engine's JSON dialect.
//!
//! Docker and `wslc` print one compact JSON object per line with `--format
//! json`; Podman prints `{{json .}}` per line and has no populated `Status` or
//! `Ports` in that projection, so the snapshot command appends them as extra
//! tab-separated columns. Rows are split on the JSON object boundary rather
//! than on `\t`, so a crafted field can never shift the appended columns.

use serde_json::{Map, Value};

use crate::container::model::{
    Container, ContainerSnapshot, ContainerStatus, Image, Network, Pod, Volume,
};

const ROW_PREFIX_CONTAINER: &str = "CTR";
const ROW_PREFIX_IMAGE: &str = "IMG";
const ROW_PREFIX_VOLUME: &str = "VOL";
const ROW_PREFIX_NETWORK: &str = "NET";
const ROW_PREFIX_POD: &str = "POD";
/// Host PID per container, captured separately because Docker's `ps` context
/// has no PID field.
const ROW_PREFIX_PID: &str = "PID";
const MARKER_UNAVAILABLE: &str = "UNAVAILABLE";
const MARKER_ERROR: &str = "ERR";
/// One inventory list failed inside an otherwise successful snapshot command.
const MARKER_SECTION_ERROR: &str = "SECERR";

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
enum RowKind {
    Container,
    Image,
    Volume,
    Network,
    Pod,
}

impl RowKind {
    fn from_prefix(prefix: &str) -> Option<Self> {
        match prefix {
            ROW_PREFIX_CONTAINER => Some(Self::Container),
            ROW_PREFIX_IMAGE => Some(Self::Image),
            ROW_PREFIX_VOLUME => Some(Self::Volume),
            ROW_PREFIX_NETWORK => Some(Self::Network),
            ROW_PREFIX_POD => Some(Self::Pod),
            _ => None,
        }
    }
}

pub fn parse_snapshot(output: &str) -> ContainerSnapshot {
    let Some(section) = extract_section(output, "CONTAINER") else {
        return ContainerSnapshot::default();
    };

    let mut snapshot = ContainerSnapshot::default();
    let mut section_error: Option<String> = None;
    let mut buckets: std::collections::BTreeMap<RowKind, Vec<&str>> =
        std::collections::BTreeMap::new();
    let mut host_pids: Vec<(&str, u32)> = Vec::new();

    for line in section
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
    {
        match line {
            MARKER_UNAVAILABLE => {
                return ContainerSnapshot {
                    status: ContainerStatus::Unavailable { detail: None },
                    ..ContainerSnapshot::default()
                };
            }
            MARKER_ERROR => return error_snapshot("Container command failed."),
            _ => {}
        }
        if let Some(detail) = line.strip_prefix(MARKER_UNAVAILABLE) {
            let detail = clean_detail(detail);
            return ContainerSnapshot {
                status: ContainerStatus::Unavailable { detail },
                ..ContainerSnapshot::default()
            };
        }
        if let Some(message) = line.strip_prefix(MARKER_ERROR) {
            return error_snapshot(
                &clean_detail(message).unwrap_or_else(|| "Container command failed.".to_string()),
            );
        }
        if let Some(detail) = line.strip_prefix(MARKER_SECTION_ERROR) {
            // One list failed while the rest of the snapshot succeeded; without
            // this the section would render as a valid empty list.
            let mut parts = detail.trim_start_matches('\t').split('\t');
            let section = parts.next().unwrap_or("listing").trim();
            let code = parts.next().unwrap_or("").trim();
            if section_error.is_none() {
                section_error = Some(if code.is_empty() {
                    format!("{section} listing failed.")
                } else {
                    format!("{section} listing failed (exit {code}).")
                });
            }
            continue;
        }

        let Some((prefix, payload)) = split_row(line) else {
            continue;
        };
        if prefix == ROW_PREFIX_PID {
            let mut columns = payload.split('\t');
            let Some(id) = columns.next().map(str::trim) else {
                continue;
            };
            let Some(pid) = columns
                .next()
                .map(str::trim)
                .and_then(|pid| pid.parse::<u32>().ok())
                .filter(|pid| *pid > 0)
            else {
                continue;
            };
            host_pids.push((id, pid));
            continue;
        }
        if let Some(kind) = RowKind::from_prefix(prefix) {
            buckets.entry(kind).or_default().push(payload);
        }
    }

    let mut container_rows = 0usize;
    let mut parsed_containers = 0usize;
    for (kind, payloads) in buckets {
        for (json, extra) in record_payloads(&payloads) {
            match kind {
                RowKind::Container => {
                    container_rows += 1;
                    if let Some(container) = parse_container(&json, &extra) {
                        parsed_containers += 1;
                        snapshot.containers.push(container);
                    }
                }
                RowKind::Image => {
                    if let Some(image) = parse_image(&json) {
                        snapshot.images.push(image);
                    }
                }
                RowKind::Volume => {
                    if let Some(volume) = parse_volume(&json) {
                        snapshot.volumes.push(volume);
                    }
                }
                RowKind::Network => {
                    if let Some(network) = parse_network(&json) {
                        snapshot.networks.push(network);
                    }
                }
                RowKind::Pod => {
                    if let Some(pod) = parse_pod(&json, &extra) {
                        snapshot.pods.push(pod);
                    }
                }
            }
        }
    }

    for container in &mut snapshot.containers {
        if container.pid.is_some() {
            continue;
        }
        container.pid = host_pids
            .iter()
            .find(|(id, _)| container_id_matches(&container.id, id))
            .map(|(_, pid)| *pid);
    }

    snapshot.status = if let Some(message) = section_error {
        ContainerStatus::Error { message }
    } else if container_rows > 0 && parsed_containers == 0 {
        // Rows arrived but none matched the expected shape: surface it instead
        // of showing a misleading empty list.
        ContainerStatus::Error {
            message: "Container output could not be parsed.".to_string(),
        }
    } else {
        ContainerStatus::Available
    };
    snapshot
}

/// Parses one section's payload lines.
///
/// Docker and `wslc` print one JSON object per line (plus optional tab columns
/// for appended fields); Podman prints a single JSON array. Engine warnings
/// share the stream, so anything that is not part of the JSON payload is
/// dropped instead of being counted as a broken row.
fn record_payloads(payloads: &[&str]) -> Vec<(String, String)> {
    // Array mode only when the section itself starts with `[`; a JSON object
    // legitimately contains arrays (Names, Labels, subnets), and treating
    // those as the section payload would silently drop every row.
    let array_start = payloads
        .iter()
        .position(|payload| payload.trim_start().starts_with('['));
    let joined = array_start
        .map(|start| payloads[start..].join("\n"))
        .unwrap_or_default();
    if let (Some(start), Some(end)) = (joined.find('['), joined.rfind(']'))
        && start < end
    {
        if let Ok(serde_json::Value::Array(entries)) =
            serde_json::from_str::<serde_json::Value>(&joined[start..=end])
        {
            return entries
                .into_iter()
                .filter(|entry| entry.is_object())
                .filter_map(|entry| serde_json::to_string(&entry).ok())
                .map(|json| (json, String::new()))
                .collect();
        }
    }
    payloads
        .iter()
        .filter_map(|payload| {
            let (json, rest) = split_json_prefix(payload)?;
            Some((json.to_string(), rest.trim_start_matches('\t').to_string()))
        })
        .collect()
}

fn error_snapshot(message: &str) -> ContainerSnapshot {
    ContainerSnapshot {
        status: ContainerStatus::Error {
            message: message.to_string(),
        },
        ..ContainerSnapshot::default()
    }
}

fn clean_detail(raw: &str) -> Option<String> {
    let raw = raw.trim();
    if raw.is_empty() {
        return None;
    }
    Some(raw.chars().take(200).collect())
}

/// Splits `<PREFIX>\t<json…>` without trusting the JSON to avoid tabs.
fn split_row(line: &str) -> Option<(&str, &str)> {
    let (prefix, rest) = line.split_once('\t')?;
    Some((prefix.trim(), rest))
}

/// Finds the end of the first JSON object in `line`, honoring strings and
/// escapes, and returns the object plus whatever follows it.
pub fn split_json_prefix(line: &str) -> Option<(&str, &str)> {
    let line = line.trim_start();
    if !line.starts_with('{') {
        return None;
    }
    let bytes = line.as_bytes();
    let mut depth = 0usize;
    let mut in_string = false;
    let mut escaped = false;
    for (index, byte) in bytes.iter().enumerate() {
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
            b'{' | b'[' => depth += 1,
            b'}' | b']' => {
                depth = depth.saturating_sub(1);
                if depth == 0 {
                    return Some((&line[..=index], &line[index + 1..]));
                }
            }
            _ => {}
        }
    }
    None
}

/// Container ids are full digests from `--no-trunc`/`inspect` on one engine
/// and 12-character abbreviations from `stats`; either side may be the
/// shorter form, so a prefix match is only accepted once both are long enough
/// to be unambiguous.
fn container_id_matches(left: &str, right: &str) -> bool {
    let left = left.trim();
    let right = right.trim();
    left.eq_ignore_ascii_case(right)
        || (left.len() >= 12
            && right.len() >= 12
            && (left.starts_with(right) || right.starts_with(left)))
}

fn parse_container(json: &str, extra: &str) -> Option<Container> {
    let value = parse_object(json)?;
    let extra: Vec<&str> = extra.split('\t').collect();
    let id = string_field(&value, &["ID", "Id"])?;
    let name = first_string_field(&value, &["Names", "Name"])?;
    let image = string_field(&value, &["Image"]).unwrap_or_default();
    let appended_status = extra.first().map(|value| value.trim()).unwrap_or("");
    let status = first_non_empty_string(&value, &["Status"]).unwrap_or_else(|| {
        if appended_status.is_empty() {
            String::new()
        } else {
            appended_status.to_string()
        }
    });
    // Docker before 20.10 has no `State` in its `ps` context and wslc may omit
    // it, but the human status line still carries the state that drives action
    // availability.
    let state = string_field(&value, &["State"])
        .filter(|state| !state.trim().is_empty())
        .unwrap_or_else(|| state_from_status(&status));
    let ports = first_non_empty_string(&value, &["Ports"]).or_else(|| {
        extra
            .get(1)
            .map(|ports| ports.trim().to_string())
            .filter(|ports| !ports.is_empty())
    });
    // Podman projects an empty `CreatedAt` in template mode and only the ISO
    // `Created`; the appended running-for column keeps the human age.
    let pid = extra
        .get(3)
        .map(|pid| pid.trim())
        .and_then(|pid| pid.parse::<u32>().ok())
        .filter(|pid| *pid > 0);
    let created = extra
        .get(2)
        .map(|created| created.trim().to_string())
        .filter(|created| !created.is_empty())
        .or_else(|| first_non_empty_string(&value, &["RunningFor", "CreatedAt", "Created"]));
    Some(Container {
        id,
        name,
        image,
        state,
        status,
        ports,
        created,
        pid,
    })
}

/// Maps a human status line (`Up 2 hours (Paused)`, `Exited (0) 3 days ago`)
/// onto the engine state vocabulary used by action availability.
fn state_from_status(status: &str) -> String {
    let status = status.trim().to_ascii_lowercase();
    if status.is_empty() {
        return String::new();
    }
    if status.contains("(paused)") {
        return "paused".to_string();
    }
    let first = status
        .split(|character: char| character.is_whitespace() || character == '(')
        .next()
        .unwrap_or_default();
    match first {
        "up" => "running".to_string(),
        "exited" | "stopped" => "exited".to_string(),
        "created" | "configured" => "created".to_string(),
        "restarting" => "restarting".to_string(),
        "removing" => "removing".to_string(),
        "dead" => "dead".to_string(),
        "paused" => "paused".to_string(),
        _ => String::new(),
    }
}

fn parse_image(json: &str) -> Option<Image> {
    let value = parse_object(json)?;
    let id = string_field(&value, &["ID", "Id"])?;
    let mut repository = string_field(&value, &["Repository", "repository"]).unwrap_or_default();
    let mut tag = string_field(&value, &["Tag", "tag"]).unwrap_or_default();
    if repository.is_empty() || repository == "<none>" {
        if let Some(reference) = first_string_field(&value, &["RepoTags", "Names"]) {
            if let Some((repo, parsed_tag)) = split_reference(&reference) {
                repository = repo;
                if tag.is_empty() {
                    tag = parsed_tag;
                }
            }
        }
    }
    let size = first_non_empty_string(&value, &["Size", "size"]).unwrap_or_default();
    let created = first_non_empty_string(
        &value,
        &["CreatedSince", "CreatedAt", "CreatedTime", "Created"],
    )
    .unwrap_or_default();
    // Numeric fields stay raw for display, but sorting and humanized dates
    // need the parsed values. Docker formats both (`123MB`, a date string), so
    // the formatted text is parsed too and the sort buttons keep working.
    let size_bytes = numeric_field(&value, &["Size", "size"]).or_else(|| {
        first_non_empty_string(&value, &["Size", "size"]).and_then(|text| parse_size_bytes(&text))
    });
    let created_at = numeric_field(
        &value,
        &["Created", "CreatedAt", "CreatedTime", "CreatedSince"],
    )
    // Out-of-range values stay absent instead of wrapping into a negative
    // timestamp that would sort to the wrong end.
    .and_then(|value| i64::try_from(value).ok())
    .or_else(|| {
        // `CreatedSince` is a relative age ("3 weeks ago"), which no absolute
        // parser can turn into unix seconds.
        first_non_empty_string(&value, &["Created", "CreatedAt", "CreatedTime"])
            .and_then(|text| parse_timestamp_seconds(&text))
    });
    Some(Image {
        id,
        repository,
        tag,
        size,
        created,
        size_bytes,
        created_at,
    })
}

fn parse_volume(json: &str) -> Option<Volume> {
    let value = parse_object(json)?;
    let name = string_field(&value, &["Name", "name"])?;
    let driver = string_field(&value, &["Driver", "driver"]).unwrap_or_default();
    let mount_point =
        string_field(&value, &["Mountpoint", "MountPoint", "mountpoint"]).unwrap_or_default();
    Some(Volume {
        name,
        driver,
        mount_point,
    })
}

fn parse_network(json: &str) -> Option<Network> {
    let value = parse_object(json)?;
    let name = string_field(&value, &["Name", "name"])?;
    let id = string_field(&value, &["ID", "Id", "id"]).unwrap_or_default();
    let driver = string_field(&value, &["Driver", "driver"]).unwrap_or_default();
    let scope = string_field(&value, &["Scope", "scope"]).unwrap_or_default();
    let subnets = value
        .get("subnets")
        .and_then(Value::as_array)
        .map(|entries| {
            entries
                .iter()
                .filter_map(|entry| match entry {
                    Value::String(subnet) => Some(subnet.clone()),
                    Value::Object(object) => object
                        .get("subnet")
                        .and_then(Value::as_str)
                        .map(str::to_string),
                    _ => None,
                })
                .collect()
        })
        .unwrap_or_default();
    Some(Network {
        id,
        name,
        driver,
        scope,
        subnets,
    })
}

fn parse_pod(json: &str, extra: &str) -> Option<Pod> {
    let value = parse_object(json)?;
    let id = string_field(&value, &["Id", "ID"])?;
    let name = string_field(&value, &["Name"])?;
    let status = string_field(&value, &["Status"]).unwrap_or_default();
    let containers = value
        .get("Containers")
        .map(|containers| match containers {
            Value::Array(entries) => entries.len(),
            Value::Number(number) => number.as_u64().unwrap_or_default() as usize,
            _ => 0,
        })
        .unwrap_or_default();
    let created = extra
        .split('\t')
        .next()
        .map(str::trim)
        .filter(|created| !created.is_empty())
        .map(str::to_string);
    Some(Pod {
        id,
        name,
        status,
        created,
        containers,
    })
}

fn parse_object(json: &str) -> Option<Map<String, Value>> {
    match serde_json::from_str::<Value>(json).ok()? {
        Value::Object(object) => Some(object),
        _ => None,
    }
}

fn field<'a>(object: &'a Map<String, Value>, names: &[&str]) -> Option<&'a Value> {
    // Names are ordered by preference: `RunningFor` must win over `CreatedAt`
    // no matter how the engine sorts its JSON keys.
    names.iter().find_map(|name| {
        object
            .iter()
            .find(|(key, _)| key.eq_ignore_ascii_case(name))
            .map(|(_, value)| value)
    })
}

fn first_non_empty_string(object: &Map<String, Value>, names: &[&str]) -> Option<String> {
    names.iter().find_map(|name| {
        let value = object
            .iter()
            .find(|(key, _)| key.eq_ignore_ascii_case(name))
            .map(|(_, value)| value)?;
        let text = match value {
            Value::String(text) => text.clone(),
            Value::Number(number) => number.to_string(),
            _ => return None,
        };
        (!text.trim().is_empty()).then_some(text)
    })
}

/// Reads a numeric field that engines emit either as a JSON number or as a
/// bare digit string.
fn numeric_field(object: &Map<String, Value>, names: &[&str]) -> Option<u64> {
    names.iter().find_map(|name| {
        let value = object
            .iter()
            .find(|(key, _)| key.eq_ignore_ascii_case(name))
            .map(|(_, value)| value)?;
        match value {
            Value::Number(number) => number.as_u64(),
            Value::String(text) => text.trim().parse().ok(),
            _ => None,
        }
    })
}

/// Reads the engine-formatted size (`123MB`, `41.2 MB`, `1.5GiB`, `0B`) into
/// bytes so the image table can sort without re-parsing display text.
pub(crate) fn parse_size_bytes(raw: &str) -> Option<u64> {
    let text = raw.trim();
    let split = text.find(|character: char| character.is_ascii_alphabetic())?;
    let (number, unit) = text.split_at(split);
    let value: f64 = number.trim().parse().ok()?;
    let unit = unit.trim().to_ascii_lowercase();
    // Docker and Podman print decimal units; binary spellings show up in some
    // engine versions, so both are accepted.
    let multiplier = match unit.as_str() {
        "b" => 1f64,
        "kb" => 1_000f64,
        "kib" => 1_024f64,
        "mb" => 1_000_000f64,
        "mib" => 1_048_576f64,
        "gb" => 1_000_000_000f64,
        "gib" => 1_073_741_824f64,
        "tb" => 1_000_000_000_000f64,
        "tib" => 1_099_511_627_776f64,
        _ => return None,
    };
    let bytes = value * multiplier;
    if !bytes.is_finite() || bytes < 0f64 || bytes >= u64::MAX as f64 {
        return None;
    }
    Some(bytes.round() as u64)
}

/// Parses the creation time Docker formats
/// (`2024-01-02 03:04:05 +0800 CST`) and the RFC3339 form other engines emit
/// into unix seconds.
fn parse_timestamp_seconds(raw: &str) -> Option<i64> {
    let text = raw.trim();
    if text.is_empty() {
        return None;
    }
    // Podman already emits unix seconds, sometimes as a string.
    if let Ok(seconds) = text.parse::<i64>() {
        return Some(seconds);
    }
    // Split on the space first: Docker's zone suffix (`CST`) contains a `T`
    // that would otherwise be mistaken for the ISO date separator.
    let (date, rest) = text.split_once(' ').or_else(|| text.split_once('T'))?;
    if text.matches('-').count() < 2 {
        return None;
    }
    let (year, month, day) = parse_date(date)?;
    let (hour, minute, second) = parse_clock(rest)?;
    let offset = parse_offset(rest)?;
    Some(
        days_from_civil(year, month, day)?
            .checked_mul(86_400)?
            .checked_add(hour * 3600 + minute * 60 + second)?
            .checked_sub(offset)?,
    )
}

fn parse_date(date: &str) -> Option<(i64, i64, i64)> {
    let mut parts = date.split('-');
    let year: i64 = parts.next()?.parse().ok()?;
    let month: i64 = parts.next()?.parse().ok()?;
    let day: i64 = parts.next()?.parse().ok()?;
    // Four-digit years cover every engine timestamp; the bound also keeps the
    // civil-date conversion from overflowing.
    if parts.next().is_some()
        || !(1..=9999).contains(&year)
        || !(1..=12).contains(&month)
        || !(1..=days_in_month(year, month)).contains(&day)
    {
        return None;
    }
    Some((year, month, day))
}

fn days_in_month(year: i64, month: i64) -> i64 {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if (year % 4 == 0 && year % 100 != 0) || year % 400 == 0 => 29,
        2 => 28,
        _ => 0,
    }
}

/// Reads `HH:MM:SS`, ignoring RFC3339 fractional seconds and a trailing offset
/// glued to the clock (`03:04:05+08:00`).
fn parse_clock(rest: &str) -> Option<(i64, i64, i64)> {
    let clock = rest.split_whitespace().next()?;
    let clock = match clock.find(|character: char| {
        character == '+' || character == '-' || character == 'Z' || character == 'z'
    }) {
        Some(position) => &clock[..position],
        None => clock,
    };
    let clock = clock.split('.').next()?;
    let mut parts = clock.split(':');
    let hour: i64 = parts.next()?.parse().ok()?;
    let minute: i64 = parts.next()?.parse().ok()?;
    let second: i64 = parts.next()?.parse().ok()?;
    if !(0..24).contains(&hour) || !(0..60).contains(&minute) || !(0..=60).contains(&second) {
        return None;
    }
    Some((hour, minute, second))
}

/// Offset in seconds east of UTC from `Z`, `+0800` or `+08:00`.
fn parse_offset(rest: &str) -> Option<i64> {
    // The offset is the last `+`, `-`, `Z` or `z` in the text: ISO dates carry
    // earlier hyphens, and Docker appends a zone name after the digits
    // (`+0800 CST`). A candidate only counts when what follows is a numeric
    // offset (four digits, optional colon) or an optional alphabetic zone.
    let rest = rest.trim();
    let mut candidate = None;
    for (index, character) in rest.char_indices().rev() {
        match character {
            'Z' | 'z' => {
                let tail = rest[index + character.len_utf8()..].trim();
                if tail
                    .chars()
                    .all(|character| character.is_ascii_alphabetic())
                {
                    candidate = Some(index);
                    break;
                }
            }
            '+' | '-' => {
                let tail = rest[index + 1..].trim_start();
                let digits: String = tail
                    .chars()
                    .take_while(|character| character.is_ascii_digit() || *character == ':')
                    .collect();
                if digits.replace(':', "").len() == 4 {
                    candidate = Some(index);
                    break;
                }
            }
            _ => {}
        }
    }
    let text = &rest[candidate?..];
    if text.eq_ignore_ascii_case("z") {
        return Some(0);
    }
    let (sign, digits) = match text.strip_prefix('+') {
        Some(digits) => (1i64, digits),
        None => (-1i64, text.strip_prefix('-')?),
    };
    let digits: String = digits
        .chars()
        .filter(|character| character.is_ascii_digit())
        .collect();
    if digits.len() != 4 {
        return None;
    }
    let hours: i64 = digits[..2].parse().ok()?;
    let minutes: i64 = digits[2..].parse().ok()?;
    if hours > 23 || minutes > 59 {
        return None;
    }
    Some(sign * (hours * 3600 + minutes * 60))
}

/// Days since 1970-01-01 for a proleptic Gregorian date (Howard Hinnant's
/// `days_from_civil`), which keeps a date crate out of the parser.
fn days_from_civil(year: i64, month: i64, day: i64) -> Option<i64> {
    let year = if month <= 2 { year - 1 } else { year };
    let era = if year >= 0 { year } else { year - 399 } / 400;
    let year_of_era = year - era * 400;
    let shifted_month = (month + 9) % 12;
    let day_of_year = (153 * shifted_month + 2) / 5 + day - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    era.checked_mul(146_097)?
        .checked_add(day_of_era)?
        .checked_sub(719_468)
}

fn string_field(object: &Map<String, Value>, names: &[&str]) -> Option<String> {
    field(object, names).and_then(|value| match value {
        Value::String(text) => Some(text.clone()),
        Value::Number(number) => Some(number.to_string()),
        _ => None,
    })
}

fn first_string_field(object: &Map<String, Value>, names: &[&str]) -> Option<String> {
    let value = field(object, names)?;
    match value {
        Value::String(text) => Some(text.clone()),
        Value::Array(entries) => entries
            .iter()
            .find_map(|entry| entry.as_str().map(str::to_string)),
        _ => None,
    }
}

fn split_reference(reference: &str) -> Option<(String, String)> {
    let reference = reference.trim();
    if reference.is_empty() {
        return None;
    }
    match reference.rsplit_once(':') {
        Some((repository, tag)) if !tag.contains('/') => {
            Some((repository.to_string(), tag.to_string()))
        }
        _ => Some((reference.to_string(), String::new())),
    }
}

fn extract_section<'a>(output: &'a str, name: &str) -> Option<&'a str> {
    let start = format!("==={name}===");
    let end = format!("==={name}_END===");
    let after_start = output.split_once(&start)?.1;
    // A capture capped mid-stream loses the end marker; the rows that did
    // arrive are still valid, so parse up to the truncation instead of
    // discarding the whole inventory.
    Some(match after_start.split_once(&end) {
        Some((section, _)) => section.trim(),
        None => after_start.trim(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn docker_lines_keep_status_and_ports_from_json() {
        let output = concat!(
            "===CONTAINER===\n",
            "CTR\t{\"Command\":\"\\\"sleep 300\\\"\",\"CreatedAt\":\"2026-10-09 10:37:29 +0800 GMT+8\",",
            "\"ID\":\"19ff1a78d092\",\"Image\":\"alpine:latest\",\"Names\":\"uc-cli-pub\",",
            "\"Ports\":\"0.0.0.0:8080->80/tcp\",\"RunningFor\":\"2 weeks ago\",",
            "\"State\":\"created\",\"Status\":\"Created\"}\n",
            "IMG\t{\"ID\":\"320994c3b997\",\"Repository\":\"alpine\",\"Tag\":\"latest\",",
            "\"Size\":\"8.42MB\",\"CreatedSince\":\"5 weeks ago\"}\n",
            "VOL\t{\"Name\":\"app-data\",\"Driver\":\"local\",\"Mountpoint\":\"/var/lib/docker/volumes/app-data/_data\"}\n",
            "NET\t{\"ID\":\"73bb526fbe81\",\"Name\":\"bridge\",\"Driver\":\"bridge\",\"Scope\":\"local\"}\n",
            "===CONTAINER_END===",
        );
        let snapshot = parse_snapshot(output);
        assert_eq!(snapshot.status, ContainerStatus::Available);
        assert_eq!(snapshot.containers.len(), 1);
        let container = &snapshot.containers[0];
        assert_eq!(container.id, "19ff1a78d092");
        assert_eq!(container.name, "uc-cli-pub");
        assert_eq!(container.ports.as_deref(), Some("0.0.0.0:8080->80/tcp"));
        assert_eq!(container.status, "Created");
        assert_eq!(container.created.as_deref(), Some("2 weeks ago"));
        assert_eq!(snapshot.images[0].repository, "alpine");
        assert_eq!(snapshot.images[0].tag, "latest");
        assert_eq!(snapshot.volumes[0].name, "app-data");
        assert_eq!(snapshot.networks[0].name, "bridge");
        assert!(snapshot.networks[0].subnets.is_empty());
    }

    #[test]
    fn podman_lines_take_appended_status_ports_and_pod_created() {
        let output = concat!(
            "===CONTAINER===\n",
            "CTR\t{\"Id\":\"42bac06d921bbf44e8c9d3a0d3b9a7dee566423ecce71688aa24bd0cb1deb356\",",
            "\"Names\":[\"vigilant_meninsky\"],\"Image\":\"docker.io/mlan/openldap:latest\",",
            "\"State\":\"exited\",\"Status\":\"\",\"Ports\":null,\"CreatedAt\":\"2 months ago\"}",
            "\tExited (0) 2 months ago\t0.0.0.0:389->389/tcp\t2 months ago\n",
            "IMG\t{\"repository\":\"docker.io/library/redis\",\"tag\":\"7-alpine\",\"Id\":\"98c6ca8f397f\",",
            "\"Size\":\"41.2 MB\",\"CreatedAt\":\"3 weeks ago\"}\n",
            "NET\t{\"name\":\"podman\",\"id\":\"e4881cd70467\",\"driver\":\"bridge\",\"scope\":\"local\",",
            "\"subnets\":[{\"subnet\":\"10.88.0.0/16\"},{\"subnet\":\"fd00::/64\"}]}\n",
            "POD\t{\"Id\":\"3cd7df7af038e334ea5eb404719eea184303544618b797d57887c41f6ddc9ed0\",",
            "\"Name\":\"pod_ldap\",\"Status\":\"Exited\",\"Containers\":[{\"Id\":\"552d85b062e9\"}]}",
            "\t8 weeks ago\n",
            "===CONTAINER_END===",
        );
        let snapshot = parse_snapshot(output);
        assert_eq!(snapshot.status, ContainerStatus::Available);
        let container = &snapshot.containers[0];
        assert_eq!(container.name, "vigilant_meninsky");
        assert_eq!(container.status, "Exited (0) 2 months ago");
        assert_eq!(container.ports.as_deref(), Some("0.0.0.0:389->389/tcp"));
        assert_eq!(container.created.as_deref(), Some("2 months ago"));
        assert_eq!(snapshot.images[0].tag, "7-alpine");
        assert_eq!(
            snapshot.networks[0].subnets,
            vec!["10.88.0.0/16", "fd00::/64"]
        );
        assert_eq!(snapshot.pods[0].name, "pod_ldap");
        assert_eq!(snapshot.pods[0].containers, 1);
        assert_eq!(snapshot.pods[0].created.as_deref(), Some("8 weeks ago"));
    }

    #[test]
    fn missing_cli_and_failures_are_distinct() {
        let missing = parse_snapshot(
            "===CONTAINER===\nUNAVAILABLE\tdocker CLI not found\n===CONTAINER_END===",
        );
        match missing.status {
            ContainerStatus::Unavailable { detail } => {
                assert_eq!(detail.as_deref(), Some("docker CLI not found"));
            }
            other => panic!("expected unavailable, got {other:?}"),
        }
        let failed = parse_snapshot(
            "===CONTAINER===\nERR\tCannot connect to the Docker daemon\n===CONTAINER_END===",
        );
        assert_eq!(
            failed.status,
            ContainerStatus::Error {
                message: "Cannot connect to the Docker daemon".to_string()
            }
        );
        // Engine warnings and other noise are ignored; a JSON-shaped row that
        // still cannot be parsed is reported instead of silently dropped.
        let noise = parse_snapshot(
            "===CONTAINER===\nCTR\ttime=\"...\" level=warning msg=\"rootless\"\n===CONTAINER_END===",
        );
        assert_eq!(noise.status, ContainerStatus::Available);
        let unparseable = parse_snapshot("===CONTAINER===\nCTR\t{\"State\":}\n===CONTAINER_END===");
        assert!(matches!(unparseable.status, ContainerStatus::Error { .. }));
    }

    #[test]
    fn failed_inventory_sections_are_errors_not_empty_lists() {
        let output = concat!(
            "===CONTAINER===\n",
            "CTR\t{\"ID\":\"a1\",\"Names\":\"web\",\"Image\":\"nginx\",\"State\":\"running\"}\n",
            "SECERR\timages\t1\n",
            "SECERR\tvolumes\t125\n",
            "===CONTAINER_END===",
        );
        let snapshot = parse_snapshot(output);
        assert_eq!(
            snapshot.status,
            ContainerStatus::Error {
                message: "images listing failed (exit 1).".to_string()
            }
        );
        // Rows from the lists that did answer are still parsed.
        assert_eq!(snapshot.containers.len(), 1);
        assert!(snapshot.images.is_empty());
        assert!(snapshot.volumes.is_empty());
    }

    #[test]
    fn split_json_prefix_ignores_tabs_and_braces_inside_strings() {
        let line = "{\"Ports\":\"a\tb\",\"Nested\":{\"x\":1}}\tUp 2 minutes\t80/tcp";
        let (json, rest) = split_json_prefix(line).expect("json prefix");
        assert_eq!(json, "{\"Ports\":\"a\tb\",\"Nested\":{\"x\":1}}");
        assert_eq!(rest, "\tUp 2 minutes\t80/tcp");
        assert!(split_json_prefix("not json").is_none());
        assert!(split_json_prefix("{\"unterminated\":true").is_none());
    }

    #[test]
    fn image_metadata_is_parsed_for_sorting_and_humanized_display() {
        // Podman prints bytes and unix seconds; both must become numbers so the
        // viewer can order and format them without re-parsing display text.
        let output = concat!(
            "===CONTAINER===\n",
            "IMG\t{\"Id\":\"abc\",\"RepoTags\":[\"nginx:alpine\"],\"Size\":39875426,\"Created\":1793032819}\n",
            "===CONTAINER_END===",
        );
        let snapshot = parse_snapshot(output);
        assert_eq!(snapshot.images.len(), 1);
        let image = &snapshot.images[0];
        assert_eq!(image.size, "39875426");
        assert_eq!(image.size_bytes, Some(39_875_426));
        assert_eq!(image.created_at, Some(1_793_032_819));
    }

    #[test]
    fn docker_formatted_image_size_and_timestamp_become_sortable() {
        // Real `docker images --format '{{json .}}'` shape: size and creation
        // time are human strings, so both sort keys used to stay `None`.
        let output = concat!(
            "===CONTAINER===\n",
            "IMG\t{\"Containers\":\"N/A\",\"CreatedAt\":\"2024-01-02 03:04:05 +0800 CST\",",
            "\"CreatedSince\":\"2 years ago\",\"ID\":\"320994c3b997\",\"Repository\":\"redis\",",
            "\"SharedSize\":\"N/A\",\"Size\":\"123MB\",\"Tag\":\"7\",\"UniqueSize\":\"N/A\"}\n",
            "IMG\t{\"CreatedAt\":\"2024-01-02T03:04:05Z\",\"ID\":\"def\",\"RepoTags\":[\"alpine:3.20\"],",
            "\"Size\":\"1.5GiB\"}\n",
            "===CONTAINER_END===",
        );
        let snapshot = parse_snapshot(output);
        assert_eq!(snapshot.images.len(), 2);
        let redis = &snapshot.images[0];
        assert_eq!(redis.size, "123MB");
        assert_eq!(redis.size_bytes, Some(123_000_000));
        assert_eq!(redis.created, "2 years ago");
        assert_eq!(redis.created_at, Some(1_704_135_845));
        let alpine = &snapshot.images[1];
        assert_eq!(alpine.size, "1.5GiB");
        assert_eq!(alpine.size_bytes, Some(1_610_612_736));
        assert_eq!(alpine.created_at, Some(1_704_164_645));
    }

    #[test]
    fn image_metadata_that_cannot_be_parsed_stays_absent() {
        // Relative ages and non-numeric sizes keep their display text and no
        // invented sort key.
        let output = concat!(
            "===CONTAINER===\n",
            "IMG\t{\"Id\":\"def\",\"RepoTags\":[\"redis:7\"],\"Size\":\"N/A\",\"CreatedSince\":\"3 weeks ago\"}\n",
            "===CONTAINER_END===",
        );
        let snapshot = parse_snapshot(output);
        let image = &snapshot.images[0];
        assert_eq!(image.size_bytes, None);
        assert_eq!(image.created_at, None);
        assert_eq!(image.created, "3 weeks ago");
    }

    #[test]
    fn host_pids_merge_by_id_and_short_form() {
        let output = concat!(
            "===CONTAINER===\n",
            "CTR\t{\"ID\":\"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\",\"Names\":\"web\",\"Image\":\"nginx\"}\n",
            "PID\taaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\t4242\n",
            // Podman reports the pid in the ps template's extra column.
            "CTR\t{\"Id\":\"bbbbbbbbbbbb\",\"Name\":\"api\",\"Image\":\"app\"}\tUp\t\t\t777\n",
            "===CONTAINER_END===",
        );
        let snapshot = parse_snapshot(output);
        assert_eq!(snapshot.containers[0].pid, Some(4242));
        assert_eq!(snapshot.containers[1].pid, Some(777));
    }

    #[test]
    fn stopped_containers_report_no_host_pid() {
        let output = concat!(
            "===CONTAINER===\n",
            "CTR\t{\"Id\":\"bbbbbbbbbbbb\",\"Name\":\"api\",\"Image\":\"app\"}\tUp\t\t\t0\n",
            "PID\tbbbbbbbbbbbb\t0\n",
            "===CONTAINER_END===",
        );
        let snapshot = parse_snapshot(output);
        assert_eq!(snapshot.containers[0].pid, None);
    }

    #[test]
    fn size_and_timestamp_parsers_cover_their_formatted_shapes() {
        assert_eq!(parse_size_bytes("0B"), Some(0));
        assert_eq!(parse_size_bytes("41 MB"), Some(41_000_000));
        assert_eq!(parse_size_bytes("41.2MB"), Some(41_200_000));
        assert_eq!(parse_size_bytes("1.5 GiB"), Some(1_610_612_736));
        assert_eq!(parse_size_bytes("2TB"), Some(2_000_000_000_000));
        assert_eq!(parse_size_bytes("N/A"), None);
        assert_eq!(parse_size_bytes("-5MB"), None);

        assert_eq!(
            parse_timestamp_seconds("2026-10-22 10:11:12 +0800 CST"),
            Some(1_792_635_072)
        );
        assert_eq!(
            parse_timestamp_seconds("2024-01-02T03:04:05.123+08:00"),
            Some(1_704_135_845)
        );
        assert_eq!(parse_timestamp_seconds("1793032819"), Some(1_793_032_819));
        assert_eq!(parse_timestamp_seconds("3 weeks ago"), None);
    }

    #[test]
    fn images_without_repository_fall_back_to_their_tags() {
        let output = concat!(
            "===CONTAINER===\n",
            "IMG\t{\"Id\":\"98c6ca8f397f\",\"RepoTags\":[\"docker.io/library/redis:7-alpine\"],\"Size\":\"41 MB\"}\n",
            "===CONTAINER_END===",
        );
        let snapshot = parse_snapshot(output);
        assert_eq!(snapshot.status, ContainerStatus::Available);
        assert_eq!(snapshot.images.len(), 1);
        assert_eq!(snapshot.images[0].repository, "docker.io/library/redis");
        assert_eq!(snapshot.images[0].tag, "7-alpine");
    }
}

/// Registry descriptions can carry HTML (quay.io does); the panel renders
/// Recovers the manifest blob that `podman manifest inspect` prints in its
/// failure for single-image manifests.
///
/// Podman only renders manifest *lists*; when a registry serves a plain image
/// manifest it aborts after fetching with `parsing manifest blob "<blob>" as a
/// "<media type>": Treating single images as manifest lists is not
/// implemented`. The blob is the exact registry response, so the inspector can
/// still show it instead of that wall of escaped JSON.
pub fn recover_single_image_manifest(stderr: &str) -> Option<String> {
    const MARKER: &str = "parsing manifest blob ";
    let rest = stderr.get(stderr.find(MARKER)? + MARKER.len()..)?;
    // Podman quotes the blob with Go's `%q`, which is also a valid JSON string
    // literal, so serde unescapes it without a bespoke decoder.
    let literal = leading_json_string_literal(rest)?;
    let blob: String = serde_json::from_str(literal).ok()?;
    let value: Value = serde_json::from_str(&blob).ok()?;
    serde_json::to_string_pretty(&value).ok()
}

/// Returns the JSON string literal that starts `text`, quotes included.
fn leading_json_string_literal(text: &str) -> Option<&str> {
    let mut chars = text.char_indices();
    if chars.next()?.1 != '"' {
        return None;
    }
    let mut escaped = false;
    for (index, ch) in chars {
        match ch {
            '\\' if !escaped => escaped = true,
            '"' if !escaped => return Some(&text[..=index]),
            _ => escaped = false,
        }
    }
    None
}

#[cfg(test)]
mod manifest_tests {
    use super::*;

    #[test]
    fn recovers_single_image_manifest_from_podman_error() {
        // Real podman 6.2 output: the blob arrives Go-quoted inside the error.
        let error = r#"Error: getting content of manifest list or image docker.io/library/nginx@sha256:abc: parsing manifest blob "{\n  \"schemaVersion\": 2,\n  \"mediaType\": \"application/vnd.oci.image.manifest.v1+json\",\n  \"config\": {\"mediaType\": \"application/vnd.oci.image.config.v1+json\", \"digest\": \"sha256:def\", \"size\": 28421},\n  \"layers\": []\n}" as a "application/vnd.oci.image.manifest.v1+json": Treating single images as manifest lists is not implemented"#;
        let manifest = recover_single_image_manifest(error).expect("recovered manifest");
        let parsed: Value = serde_json::from_str(&manifest).expect("pretty manifest");
        assert_eq!(
            parsed["mediaType"],
            Value::String("application/vnd.oci.image.manifest.v1+json".into())
        );
        assert_eq!(parsed["config"]["digest"], "sha256:def");
        assert!(manifest.contains("\n"), "manifest is pretty-printed");
        // Errors that are not this limitation stay untouched.
        assert!(recover_single_image_manifest("Error: unauthorized").is_none());
        assert!(recover_single_image_manifest("parsing manifest blob not-json").is_none());
    }
}
