// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use crate::{BufferOffset, FindMatch, TextRange};

/// Ordered search ranges. Equal-length matches share their length; ordinary
/// files use 32-bit offsets without imposing a size limit on the document.
#[derive(Clone, Debug)]
pub struct FindMatches {
    storage: MatchStorage,
}

#[derive(Clone, Debug)]
enum MatchStorage {
    Compact {
        starts: Vec<u32>,
        length: usize,
        ends: Option<Vec<u32>>,
    },
    Wide(Vec<FindMatch>),
}

impl Default for FindMatches {
    fn default() -> Self {
        Self {
            storage: MatchStorage::Compact {
                starts: Vec::new(),
                length: 0,
                ends: None,
            },
        }
    }
}

impl FindMatches {
    pub fn len(&self) -> usize {
        match &self.storage {
            MatchStorage::Compact { starts, .. } => starts.len(),
            MatchStorage::Wide(matches) => matches.len(),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn get(&self, index: usize) -> Option<FindMatch> {
        match &self.storage {
            MatchStorage::Compact {
                starts,
                length,
                ends,
            } => {
                let start = *starts.get(index)? as usize;
                let end = ends
                    .as_ref()
                    .map_or(start + length, |ends| ends[index] as usize);
                Some(FindMatch {
                    range: TextRange::new(BufferOffset(start), BufferOffset(end)),
                })
            }
            MatchStorage::Wide(matches) => matches.get(index).cloned(),
        }
    }

    pub fn iter(&self) -> impl ExactSizeIterator<Item = FindMatch> + DoubleEndedIterator + '_ {
        (0..self.len()).map(|index| self.get(index).unwrap())
    }

    pub fn partition_point(&self, mut predicate: impl FnMut(FindMatch) -> bool) -> usize {
        let (mut left, mut right) = (0, self.len());
        while left < right {
            let middle = left + (right - left) / 2;
            if predicate(self.get(middle).unwrap()) {
                left = middle + 1;
            } else {
                right = middle;
            }
        }
        left
    }

    pub fn push(&mut self, hit: FindMatch) {
        if u32::try_from(hit.range.end.0).is_err()
            && matches!(self.storage, MatchStorage::Compact { .. })
        {
            self.storage = MatchStorage::Wide(self.iter().collect());
        }
        match &mut self.storage {
            MatchStorage::Compact {
                starts,
                length,
                ends,
            } => {
                let hit_length = hit.range.end.0 - hit.range.start.0;
                if starts.is_empty() {
                    *length = hit_length;
                }
                if hit_length != *length && ends.is_none() {
                    *ends = Some(
                        starts
                            .iter()
                            .map(|&start| (start as usize + *length) as u32)
                            .collect(),
                    );
                }
                starts.push(hit.range.start.0 as u32);
                if let Some(ends) = ends {
                    ends.push(hit.range.end.0 as u32);
                }
            }
            MatchStorage::Wide(matches) => matches.push(hit),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ranges_keep_order_across_variable_lengths_and_large_offsets() {
        let mut matches = FindMatches::default();
        let limit = u32::MAX as usize;
        let ranges = [2..5, 6..9, 10..15, limit..limit + 3];
        for range in &ranges {
            matches.push(FindMatch {
                range: TextRange::new(BufferOffset(range.start), BufferOffset(range.end)),
            });
        }
        assert_eq!(
            matches
                .iter()
                .map(|hit| hit.range.as_range())
                .collect::<Vec<_>>(),
            ranges
        );
        assert_eq!(matches.partition_point(|hit| hit.range.end.0 <= 9), 2);
        assert_eq!(matches.get(4), None);
    }
}
