// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

//! Transfer conflict detection independent from any UI file-list model.

use crate::TransferDirection;

#[derive(Clone, Copy, Debug)]
pub struct ConflictTransfer<'a> {
    pub name: &'a str,
    pub source_size: u64,
    pub source_modified: Option<i64>,
    pub source_is_directory: bool,
    pub direction: TransferDirection,
}

#[derive(Clone, Copy, Debug)]
pub struct ConflictTarget<'a> {
    pub name: &'a str,
    pub size: u64,
    pub modified: Option<i64>,
    pub is_directory: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TransferConflict {
    pub file_name: String,
    pub source_size: u64,
    pub source_modified: Option<i64>,
    pub target_size: u64,
    pub target_modified: Option<i64>,
    pub direction: TransferDirection,
}

pub fn find_transfer_conflicts<'a>(
    transfers: impl IntoIterator<Item = ConflictTransfer<'a>>,
    targets: impl IntoIterator<Item = ConflictTarget<'a>>,
) -> Vec<TransferConflict> {
    let targets = targets.into_iter().collect::<Vec<_>>();
    transfers
        .into_iter()
        .filter_map(|transfer| {
            let target = targets.iter().find(|target| target.name == transfer.name)?;
            Some(TransferConflict {
                file_name: transfer.name.to_string(),
                source_size: transfer.source_size,
                source_modified: transfer.source_modified,
                target_size: target.size,
                target_modified: target.modified,
                direction: transfer.direction,
            })
        })
        .collect()
}

pub fn source_not_newer_than_target<'a>(
    source_name: &str,
    source_modified: Option<i64>,
    targets: impl IntoIterator<Item = ConflictTarget<'a>>,
) -> bool {
    let Some(target) = targets
        .into_iter()
        .find(|target| target.name == source_name && !target.is_directory)
    else {
        return false;
    };
    matches!(
        (source_modified, target.modified),
        (Some(source_modified), Some(target_modified)) if source_modified <= target_modified
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_file_and_directory_name_collisions() {
        let conflicts = find_transfer_conflicts(
            [
                ConflictTransfer {
                    name: "same.txt",
                    source_size: 10,
                    source_modified: Some(20),
                    source_is_directory: false,
                    direction: TransferDirection::Upload,
                },
                ConflictTransfer {
                    name: "folder",
                    source_size: 0,
                    source_modified: Some(20),
                    source_is_directory: true,
                    direction: TransferDirection::Upload,
                },
            ],
            [
                ConflictTarget {
                    name: "same.txt",
                    size: 8,
                    modified: Some(10),
                    is_directory: false,
                },
                ConflictTarget {
                    name: "folder",
                    size: 0,
                    modified: Some(10),
                    is_directory: true,
                },
            ],
        );

        assert_eq!(conflicts.len(), 2);
        assert_eq!(conflicts[0].file_name, "same.txt");
        assert_eq!(conflicts[0].target_size, 8);
        assert_eq!(conflicts[1].file_name, "folder");
    }

    #[test]
    fn compares_age_only_when_both_timestamps_exist() {
        let target = || {
            [ConflictTarget {
                name: "same.txt",
                size: 8,
                modified: Some(20),
                is_directory: false,
            }]
        };

        assert!(source_not_newer_than_target("same.txt", Some(20), target()));
        assert!(!source_not_newer_than_target("same.txt", None, target()));
    }
}
