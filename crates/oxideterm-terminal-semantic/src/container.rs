// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

//! Container table states limited to listing and inspection commands, excluding arbitrary logs.

use crate::{SemanticClass, SemanticLineRole, command::ParsedCommand, scheme::Candidate};

const CONTAINER_STATUS_PRIORITY: u8 = 94;

pub(crate) fn output_role_for_command(command: &ParsedCommand<'_>) -> Option<SemanticLineRole> {
    let executable = command.executable();
    let mut args = command.arguments();
    let structured = match executable {
        "docker" | "podman" | "nerdctl" => match args.next() {
            Some("ps") => true,
            Some("container" | "compose") => matches!(args.next(), Some("ls" | "ps")),
            _ => false,
        },
        "kubectl" => matches!(args.next(), Some("get" | "describe")),
        _ => false,
    };
    structured.then_some(SemanticLineRole::ContainerOutput)
}

pub(crate) fn line_candidates(
    text: &str,
    role: SemanticLineRole,
    allows_class: impl Fn(SemanticClass) -> bool,
) -> Vec<Candidate> {
    if role != SemanticLineRole::ContainerOutput {
        return Vec::new();
    }
    let mut candidates = Vec::new();
    for (start, token) in token_ranges(text) {
        let label = token.trim_matches(|character: char| {
            !character.is_alphanumeric() && !matches!(character, '/' | '-')
        });
        let class = container_status_class(label)
            .or_else(|| readiness_class(label))
            .or_else(|| docker_exit_class(label, text, start + token.len()));
        let Some(class) = class else {
            continue;
        };
        if allows_class(class) {
            let label_start = start + token.find(label).expect("label belongs to token");
            candidates.push(Candidate::new(
                label_start..label_start + label.len(),
                class,
                CONTAINER_STATUS_PRIORITY,
            ));
        }
    }
    candidates
}

fn container_status_class(label: &str) -> Option<SemanticClass> {
    match label {
        "Running" | "Completed" | "Succeeded" | "Up" | "healthy" => Some(SemanticClass::Success),
        "CrashLoopBackOff" | "Error" | "Failed" | "ImagePullBackOff" | "ErrImagePull"
        | "Evicted" | "OOMKilled" | "Dead" => Some(SemanticClass::Error),
        "Pending" | "Unknown" | "Terminating" | "ContainerCreating" | "Restarting" | "Paused" => {
            Some(SemanticClass::Warning)
        }
        "Created" => Some(SemanticClass::Info),
        "NAME" | "READY" | "STATUS" | "RESTARTS" | "AGE" | "IMAGE" | "COMMAND" | "PORTS"
        | "NAMES" => Some(SemanticClass::Keyword),
        _ => None,
    }
}

fn readiness_class(label: &str) -> Option<SemanticClass> {
    let (ready, total) = label.split_once('/')?;
    let ready = ready.parse::<u32>().ok()?;
    let total = total.parse::<u32>().ok()?;
    if total == 0 {
        None
    } else if ready == total {
        Some(SemanticClass::Success)
    } else {
        Some(SemanticClass::Warning)
    }
}

fn docker_exit_class(label: &str, text: &str, token_end: usize) -> Option<SemanticClass> {
    if label != "Exited" {
        return None;
    }
    let rest = text.get(token_end..)?.trim_start();
    let code = rest
        .strip_prefix('(')?
        .split_once(')')?
        .0
        .parse::<i32>()
        .ok()?;
    if code == 0 {
        Some(SemanticClass::Warning)
    } else {
        Some(SemanticClass::Error)
    }
}

fn token_ranges(text: &str) -> impl Iterator<Item = (usize, &str)> {
    let mut search_from = 0;
    text.split_whitespace().filter_map(move |token| {
        let offset = text[search_from..].find(token)?;
        let start = search_from + offset;
        search_from = start + token.len();
        Some((start, token))
    })
}
