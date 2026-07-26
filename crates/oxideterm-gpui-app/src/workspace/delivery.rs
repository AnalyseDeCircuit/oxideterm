// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

//! Shared delivery budgets for workspace-owned background results.

use std::{
    sync::mpsc::{Receiver, TryRecvError},
    time::{Duration, Instant},
};

/// Bounds one UI-thread delivery batch by both item count and elapsed time.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::workspace) struct DeliveryBudget {
    max_items: usize,
    max_elapsed: Duration,
}

impl DeliveryBudget {
    pub(in crate::workspace) const fn new(max_items: usize, max_elapsed: Duration) -> Self {
        assert!(
            max_items > 0,
            "delivery budget must allow at least one item"
        );
        assert!(
            !max_elapsed.is_zero(),
            "delivery budget must allow a non-zero duration"
        );
        Self {
            max_items,
            max_elapsed,
        }
    }

    pub(in crate::workspace) fn allows_next(self, processed: usize, elapsed: Duration) -> bool {
        processed < self.max_items && elapsed < self.max_elapsed
    }

    pub(in crate::workspace) const fn outcome(
        self,
        processed: usize,
        elapsed: Duration,
        source_exhausted: bool,
    ) -> DrainOutcome {
        DrainOutcome {
            processed,
            backlog_remaining: !source_exhausted,
            elapsed,
        }
    }
}

/// Describes one bounded drain without retaining any message contents.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::workspace) struct DrainOutcome {
    pub processed: usize,
    pub backlog_remaining: bool,
    pub elapsed: Duration,
}

/// Values returned by a bounded standard-library channel drain.
pub(in crate::workspace) struct ChannelDrain<T> {
    pub items: Vec<T>,
    pub outcome: DrainOutcome,
}

pub(in crate::workspace) const LIFECYCLE_DELIVERY_BUDGET: DeliveryBudget =
    DeliveryBudget::new(64, Duration::from_millis(4));
pub(in crate::workspace) const USER_ACTION_DELIVERY_BUDGET: DeliveryBudget =
    DeliveryBudget::new(32, Duration::from_millis(4));

/// Drains a standard-library channel until it is empty, disconnected, or over budget.
pub(in crate::workspace) fn drain_channel<T>(
    receiver: &Receiver<T>,
    budget: DeliveryBudget,
) -> ChannelDrain<T> {
    let started_at = Instant::now();
    let mut items = Vec::new();
    let mut source_exhausted = false;

    loop {
        if !budget.allows_next(items.len(), started_at.elapsed()) {
            break;
        }
        match receiver.try_recv() {
            Ok(item) => items.push(item),
            Err(TryRecvError::Empty) => {
                source_exhausted = true;
                break;
            }
            Err(TryRecvError::Disconnected) => {
                source_exhausted = true;
                break;
            }
        }
    }

    let elapsed = started_at.elapsed();
    ChannelDrain {
        outcome: budget.outcome(items.len(), elapsed, source_exhausted),
        items,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc;

    #[test]
    fn item_limit_reports_remaining_backlog() {
        let budget = DeliveryBudget::new(2, Duration::from_secs(1));

        let outcome = budget.outcome(2, Duration::from_millis(1), false);

        assert_eq!(outcome.processed, 2);
        assert!(outcome.backlog_remaining);
        assert!(!budget.allows_next(2, Duration::from_millis(1)));
    }

    #[test]
    fn elapsed_limit_reports_remaining_backlog() {
        let budget = DeliveryBudget::new(8, Duration::from_millis(2));

        let outcome = budget.outcome(1, Duration::from_millis(2), false);

        assert!(outcome.backlog_remaining);
        assert!(!budget.allows_next(1, Duration::from_millis(2)));
    }

    #[test]
    fn exhausted_source_does_not_report_backlog() {
        let budget = DeliveryBudget::new(8, Duration::from_millis(2));

        let outcome = budget.outcome(1, Duration::from_millis(1), true);

        assert!(!outcome.backlog_remaining);
    }

    #[test]
    fn channel_drain_preserves_items_beyond_count_budget() {
        let (sender, receiver) = mpsc::channel();
        sender.send(1).unwrap();
        sender.send(2).unwrap();
        sender.send(3).unwrap();
        let budget = DeliveryBudget::new(2, Duration::from_secs(1));

        let first = drain_channel(&receiver, budget);
        let second = drain_channel(&receiver, budget);

        assert_eq!(first.items, vec![1, 2]);
        assert_eq!(second.items, vec![3]);
        assert!(first.outcome.backlog_remaining);
        assert!(!second.outcome.backlog_remaining);
    }
}
