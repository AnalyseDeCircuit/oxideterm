// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

//! Network fields selected from ip, ss, and ping command scopes.

use std::{net::IpAddr, ops::Range};

use crate::{
    SemanticClass, SemanticLineRole,
    scheme::Candidate,
    tokens::{ranges, text_at},
};

const NETWORK_FIELD_PRIORITY: u8 = 96;
const PACKET_LOSS_ERROR_PERCENT: u8 = 50;

pub(crate) fn line_candidates(
    text: &str,
    role: SemanticLineRole,
    allows_class: impl Fn(SemanticClass) -> bool,
) -> Vec<Candidate> {
    let mut candidates = Vec::new();
    match role {
        SemanticLineRole::IpOutput => {
            push_interface_heading(text, &allows_class, &mut candidates);
            push_keywords_and_states(text, role, &allows_class, &mut candidates);
            push_addresses(text, &allows_class, &mut candidates);
        }
        SemanticLineRole::SocketOutput => {
            push_keywords_and_states(text, role, &allows_class, &mut candidates);
            push_addresses(text, &allows_class, &mut candidates);
        }
        SemanticLineRole::PingOutput => {
            push_keywords_and_states(text, role, &allows_class, &mut candidates);
            push_addresses(text, &allows_class, &mut candidates);
            push_ping_assignments(text, &allows_class, &mut candidates);
            push_packet_loss(text, &allows_class, &mut candidates);
        }
        _ => {}
    }
    candidates
}

fn push_interface_heading(
    text: &str,
    allows_class: &impl Fn(SemanticClass) -> bool,
    candidates: &mut Vec<Candidate>,
) {
    let mut fields = ranges(text);
    let (Some(index), Some(interface)) = (fields.next(), fields.next()) else {
        return;
    };
    let Some(index_text) = text_at(text, &index) else {
        return;
    };
    let Some(interface_text) = text_at(text, &interface) else {
        return;
    };
    if !index_text
        .strip_suffix(':')
        .is_some_and(|value| !value.is_empty() && value.bytes().all(|byte| byte.is_ascii_digit()))
        || !interface_text.ends_with(':')
    {
        return;
    }
    if allows_class(SemanticClass::Number) {
        candidates.push(Candidate::new(
            index.start..index.end - 1,
            SemanticClass::Number,
            NETWORK_FIELD_PRIORITY,
        ));
    }
    if allows_class(SemanticClass::Variable) {
        candidates.push(Candidate::new(
            interface.start..interface.end - 1,
            SemanticClass::Variable,
            NETWORK_FIELD_PRIORITY,
        ));
    }
}

fn push_keywords_and_states(
    text: &str,
    role: SemanticLineRole,
    allows_class: &impl Fn(SemanticClass) -> bool,
    candidates: &mut Vec<Candidate>,
) {
    let mut previous_keyword = None;
    let socket_state_column = if ranges(text).next().is_some_and(|range| {
        matches!(
            &text[range],
            "tcp" | "udp" | "u_str" | "u_dgr" | "raw" | "nl"
        )
    }) {
        1
    } else {
        0
    };
    for (column, range) in ranges(text).enumerate() {
        let Some(token) = text_at(text, &range) else {
            continue;
        };
        let label = token.trim_matches(|character: char| matches!(character, ':' | ','));
        let label_start = range.start + token.find(label).unwrap_or(0);
        let label_range = label_start..label_start + label.len();

        if previous_keyword == Some("dev") {
            if allows_class(SemanticClass::Variable) {
                candidates.push(Candidate::new(
                    label_range,
                    SemanticClass::Variable,
                    NETWORK_FIELD_PRIORITY,
                ));
            }
            previous_keyword = None;
            continue;
        }
        // A state word is meaningful only in the owning command's state field.
        let state_field = (role == SemanticLineRole::IpOutput && previous_keyword == Some("state"))
            || (role == SemanticLineRole::SocketOutput && column == socket_state_column);
        if state_field && let Some(class) = network_state_class(label) {
            if allows_class(class) {
                candidates.push(Candidate::new(label_range, class, NETWORK_FIELD_PRIORITY));
            }
            previous_keyword = None;
            continue;
        }

        if is_network_keyword(label) {
            if allows_class(SemanticClass::Keyword) {
                candidates.push(Candidate::new(
                    label_range,
                    SemanticClass::Keyword,
                    NETWORK_FIELD_PRIORITY,
                ));
            }
            previous_keyword = Some(label);
            continue;
        }

        previous_keyword = None;
    }
}

fn push_addresses(
    text: &str,
    allows_class: &impl Fn(SemanticClass) -> bool,
    candidates: &mut Vec<Candidate>,
) {
    if !allows_class(SemanticClass::Address) {
        return;
    }
    for range in ranges(text) {
        let Some(address) = network_address_range(text, range) else {
            continue;
        };
        candidates.push(Candidate::new(
            address,
            SemanticClass::Address,
            NETWORK_FIELD_PRIORITY,
        ));
    }
}

fn push_ping_assignments(
    text: &str,
    allows_class: &impl Fn(SemanticClass) -> bool,
    candidates: &mut Vec<Candidate>,
) {
    for range in ranges(text) {
        let Some(token) = text_at(text, &range) else {
            continue;
        };
        let Some((key, value)) = token.split_once('=') else {
            continue;
        };
        if !matches!(key, "icmp_seq" | "ttl" | "time") {
            continue;
        }
        if allows_class(SemanticClass::Variable) {
            candidates.push(Candidate::new(
                range.start..range.start + key.len(),
                SemanticClass::Variable,
                NETWORK_FIELD_PRIORITY,
            ));
        }
        let value = value.trim_end_matches([',', ':']);
        if !value.is_empty()
            && value
                .bytes()
                .all(|byte| byte.is_ascii_digit() || byte == b'.')
            && allows_class(SemanticClass::Number)
        {
            let value_start = range.start + key.len() + 1;
            candidates.push(Candidate::new(
                value_start..value_start + value.len(),
                SemanticClass::Number,
                NETWORK_FIELD_PRIORITY,
            ));
        }
    }
}

fn push_packet_loss(
    text: &str,
    allows_class: &impl Fn(SemanticClass) -> bool,
    candidates: &mut Vec<Candidate>,
) {
    let Some(percent_end) = text.find("% packet loss") else {
        return;
    };
    let percent_start = text[..percent_end]
        .rfind(char::is_whitespace)
        .map_or(0, |index| index + 1);
    let Some(percent) = text
        .get(percent_start..percent_end)
        .filter(|value| {
            value
                .bytes()
                .all(|byte| byte.is_ascii_digit() || byte == b'.')
        })
        .and_then(|value| value.parse::<f64>().ok())
        .filter(|value| (0.0..=100.0).contains(value))
    else {
        return;
    };
    let class = if percent == 0.0 {
        SemanticClass::Success
    } else if percent >= f64::from(PACKET_LOSS_ERROR_PERCENT) {
        SemanticClass::Error
    } else {
        SemanticClass::Warning
    };
    if allows_class(class) {
        candidates.push(Candidate::new(
            percent_start..percent_end + 1,
            class,
            NETWORK_FIELD_PRIORITY,
        ));
    }
}

fn network_state_class(label: &str) -> Option<SemanticClass> {
    match label {
        "UP" | "ESTAB" | "ESTABLISHED" | "REACHABLE" => Some(SemanticClass::Success),
        "DOWN" | "FAILED" => Some(SemanticClass::Error),
        "UNKNOWN" | "SYN-SENT" | "SYN-RECV" | "CLOSE-WAIT" | "TIME-WAIT" | "STALE" => {
            Some(SemanticClass::Warning)
        }
        "LISTEN" | "UNCONN" => Some(SemanticClass::Info),
        _ => None,
    }
}

fn is_network_keyword(label: &str) -> bool {
    matches!(
        label,
        "State"
            | "Recv-Q"
            | "Send-Q"
            | "Local"
            | "Peer"
            | "Address:Port"
            | "Process"
            | "inet"
            | "inet6"
            | "brd"
            | "scope"
            | "state"
            | "mtu"
            | "qlen"
            | "default"
            | "via"
            | "dev"
            | "proto"
            | "src"
            | "metric"
            | "from"
    )
}

fn network_address_range(text: &str, range: Range<usize>) -> Option<Range<usize>> {
    let token = text_at(text, &range)?;
    let value = token.trim_matches(|character: char| matches!(character, ',' | '(' | ')'));
    let value = if is_network_address(value) {
        value
    } else {
        value
            .strip_suffix(':')
            .filter(|value| is_network_address(value))?
    };
    let value_start = range.start + token.find(value)?;

    Some(value_start..value_start + value.len())
}

fn is_network_address(value: &str) -> bool {
    if let Some((host, prefix)) = value.split_once('/') {
        let Ok(address) = host.parse::<IpAddr>() else {
            return false;
        };
        let Ok(prefix) = prefix.parse::<u8>() else {
            return false;
        };
        return prefix <= if address.is_ipv4() { 32 } else { 128 };
    }
    value.parse::<IpAddr>().is_ok()
        || value.parse::<std::net::SocketAddr>().is_ok()
        || value.rsplit_once(':').is_some_and(|(host, port)| {
            matches!(host, "*" | "0.0.0.0" | "::") && (port == "*" || port.parse::<u16>().is_ok())
        })
}
