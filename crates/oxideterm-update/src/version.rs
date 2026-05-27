// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use std::cmp::Ordering;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum VersionOrdering {
    Older,
    Equal,
    Newer,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct ParsedVersion {
    core: Vec<u64>,
    prerelease: Option<String>,
}

pub fn compare_versions(candidate: &str, current: &str) -> VersionOrdering {
    match parse_version(candidate).cmp(&parse_version(current)) {
        Ordering::Less => VersionOrdering::Older,
        Ordering::Equal => VersionOrdering::Equal,
        Ordering::Greater => VersionOrdering::Newer,
    }
}

pub fn is_update_newer(candidate: &str, current: &str) -> bool {
    compare_versions(candidate, current) == VersionOrdering::Newer
}

fn parse_version(input: &str) -> ParsedVersion {
    let trimmed = input.trim().trim_start_matches('v');
    let (core, prerelease) = trimmed
        .split_once('-')
        .map_or((trimmed, None), |(core, pre)| (core, Some(pre.to_string())));

    let mut parts = core
        .split('.')
        .map(|part| part.parse::<u64>().unwrap_or(0))
        .collect::<Vec<_>>();
    while parts.len() < 3 {
        parts.push(0);
    }

    ParsedVersion {
        core: parts,
        prerelease,
    }
}

impl Ord for ParsedVersion {
    fn cmp(&self, other: &Self) -> Ordering {
        let core_ordering = self.core.cmp(&other.core);
        if core_ordering != Ordering::Equal {
            return core_ordering;
        }

        match (&self.prerelease, &other.prerelease) {
            (None, None) => Ordering::Equal,
            (None, Some(_)) => Ordering::Greater,
            (Some(_), None) => Ordering::Less,
            (Some(left), Some(right)) => compare_prerelease(left, right),
        }
    }
}

impl PartialOrd for ParsedVersion {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

fn compare_prerelease(left: &str, right: &str) -> Ordering {
    let left_parts = left.split(['.', '-']).collect::<Vec<_>>();
    let right_parts = right.split(['.', '-']).collect::<Vec<_>>();
    let shared = left_parts.len().min(right_parts.len());

    for index in 0..shared {
        let left_part = left_parts[index];
        let right_part = right_parts[index];
        let ordering = match (left_part.parse::<u64>(), right_part.parse::<u64>()) {
            (Ok(left_num), Ok(right_num)) => left_num.cmp(&right_num),
            _ => left_part.cmp(right_part),
        };
        if ordering != Ordering::Equal {
            return ordering;
        }
    }

    left_parts.len().cmp(&right_parts.len())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compares_preview_versions_without_semver_dependency() {
        assert!(is_update_newer(
            "1.2.0-gpui-preview.2",
            "1.2.0-gpui-preview.1"
        ));
        assert!(is_update_newer("1.2.0", "1.2.0-gpui-preview.9"));
        assert!(!is_update_newer("1.2.0-beta.1", "1.2.0"));
    }
}
