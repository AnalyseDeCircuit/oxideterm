// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use crate::TransferProgress;
use std::{
    collections::{HashMap, VecDeque},
    time::{Duration, Instant},
};

const SFTP_DIRECTORY_SPEED_WINDOW: Duration = Duration::from_secs(2); // Smooth bursts from parallel file workers.
const SFTP_DIRECTORY_SPEED_SAMPLE_INTERVAL: Duration = Duration::from_millis(100); // Keep rolling history bounded at high event rates.

#[derive(Default)]
pub struct DirectoryProgressAccumulator {
    files: HashMap<(String, String), (u64, u64)>,
    transferred_bytes: u64,
    total_bytes: u64,
    speed_samples: VecDeque<(Instant, u64)>,
}

impl DirectoryProgressAccumulator {
    pub fn update(&mut self, progress: TransferProgress) -> TransferProgress {
        self.update_at(progress, Instant::now())
    }

    fn update_at(&mut self, progress: TransferProgress, now: Instant) -> TransferProgress {
        let previous_aggregate_transferred = self.transferred_bytes;
        let key = (progress.remote_path.clone(), progress.local_path.clone());
        if let Some((previous_transferred, previous_total)) = self.files.get(&key).copied() {
            self.transferred_bytes = self.transferred_bytes.saturating_sub(previous_transferred);
            self.total_bytes = self.total_bytes.saturating_sub(previous_total);
        }

        // Directory transfers can emit many file progress events; keep aggregate
        // totals incrementally instead of re-summing the whole file map per tick.
        self.transferred_bytes = self
            .transferred_bytes
            .saturating_add(progress.transferred_bytes);
        self.total_bytes = self.total_bytes.saturating_add(progress.total_bytes);
        self.files
            .insert(key, (progress.transferred_bytes, progress.total_bytes));

        if self.transferred_bytes < previous_aggregate_transferred {
            // A restarted file can make the aggregate counter move backwards.
            self.speed_samples.clear();
        }
        let speed = self.aggregate_speed(now);
        let eta_seconds = if speed > 0 && self.total_bytes > self.transferred_bytes {
            Some((self.total_bytes - self.transferred_bytes).div_ceil(speed))
        } else {
            None
        };

        TransferProgress {
            transferred_bytes: self.transferred_bytes,
            total_bytes: self.total_bytes,
            speed,
            eta_seconds,
            ..progress
        }
    }

    fn aggregate_speed(&mut self, now: Instant) -> u64 {
        let window_start = now.checked_sub(SFTP_DIRECTORY_SPEED_WINDOW);
        while self.speed_samples.len() > 1
            && window_start.is_some_and(|window_start| {
                self.speed_samples
                    .get(1)
                    .is_some_and(|(sampled_at, _)| *sampled_at <= window_start)
            })
        {
            self.speed_samples.pop_front();
        }

        let speed = self
            .speed_samples
            .front()
            .and_then(|(sampled_at, sampled_bytes)| {
                now.checked_duration_since(*sampled_at)
                    .map(|elapsed| (elapsed, *sampled_bytes))
            })
            .filter(|(elapsed, _)| !elapsed.is_zero())
            .map(|(elapsed, sampled_bytes)| {
                (self.transferred_bytes.saturating_sub(sampled_bytes) as f64
                    / elapsed.as_secs_f64()) as u64
            })
            .unwrap_or(0);

        let should_sample = self.speed_samples.back().is_none_or(|(sampled_at, _)| {
            now.checked_duration_since(*sampled_at)
                .is_some_and(|elapsed| elapsed >= SFTP_DIRECTORY_SPEED_SAMPLE_INTERVAL)
        });
        if should_sample {
            self.speed_samples.push_back((now, self.transferred_bytes));
        }

        speed
    }
}

pub(crate) async fn next_browser_progress(
    events: &mut tokio::sync::mpsc::Receiver<TransferProgress>,
    accumulator: &mut Option<DirectoryProgressAccumulator>,
) -> Option<TransferProgress> {
    let first = events.recv().await?;
    let Some(accumulator) = accumulator else {
        return Some(first);
    };
    let mut progress = accumulator.update(first);
    // Account for every file tick, but publish only the latest aggregate at the display cadence.
    let deadline = tokio::time::sleep(Duration::from_millis(100));
    tokio::pin!(deadline);
    loop {
        tokio::select! {
            event = events.recv() => match event {
                Some(event) => progress = accumulator.update(event),
                None => break,
            },
            _ = &mut deadline => break,
        }
    }
    Some(progress)
}

#[cfg(test)]
mod directory_progress_tests {
    use super::*;

    fn progress(file_name: &str, transferred_bytes: u64, total_bytes: u64) -> TransferProgress {
        TransferProgress {
            id: file_name.to_string(),
            remote_path: format!("/remote/{file_name}"),
            local_path: format!("/local/{file_name}"),
            direction: crate::TransferDirection::Download,
            state: crate::TransferState::InProgress,
            total_bytes,
            transferred_bytes,
            speed: u64::MAX,
            eta_seconds: Some(u64::MAX),
            error: None,
        }
    }

    #[test]
    fn directory_progress_uses_aggregate_byte_delta_for_speed_and_eta() {
        // Explicit timestamps keep rolling-speed tests deterministic without sleeping.
        let started_at = Instant::now();
        let mut accumulator = DirectoryProgressAccumulator::default();

        let initial = accumulator.update_at(progress("first", 100, 1_000), started_at);
        assert_eq!(initial.speed, 0);
        assert_eq!(initial.eta_seconds, None);

        let first_update = accumulator.update_at(
            progress("first", 300, 1_000),
            started_at + Duration::from_secs(1),
        );
        assert_eq!(first_update.speed, 200);
        assert_eq!(first_update.eta_seconds, Some(4));

        let parallel_update = accumulator.update_at(
            progress("second", 400, 1_000),
            started_at + Duration::from_secs(1),
        );
        assert_eq!(parallel_update.transferred_bytes, 700);
        assert_eq!(parallel_update.total_bytes, 2_000);
        assert_eq!(parallel_update.speed, 600);
        assert_eq!(parallel_update.eta_seconds, Some(3));
    }

    #[test]
    fn directory_progress_speed_uses_only_the_rolling_window() {
        let started_at = Instant::now();
        let mut accumulator = DirectoryProgressAccumulator::default();

        accumulator.update_at(progress("file", 0, 1_000), started_at);
        accumulator.update_at(
            progress("file", 100, 1_000),
            started_at + Duration::from_secs(1),
        );
        let recent = accumulator.update_at(
            progress("file", 500, 1_000),
            started_at + Duration::from_secs(3),
        );

        assert_eq!(recent.speed, 200);
        assert_eq!(recent.eta_seconds, Some(3));
    }

    #[test]
    fn directory_progress_resets_speed_when_aggregate_bytes_move_backwards() {
        let started_at = Instant::now();
        let mut accumulator = DirectoryProgressAccumulator::default();

        accumulator.update_at(progress("file", 500, 1_000), started_at);
        let progressing = accumulator.update_at(
            progress("file", 700, 1_000),
            started_at + Duration::from_secs(1),
        );
        assert_eq!(progressing.speed, 200);

        let restarted = accumulator.update_at(
            progress("file", 200, 1_000),
            started_at + Duration::from_secs(2),
        );
        assert_eq!(restarted.speed, 0);
        assert_eq!(restarted.eta_seconds, None);
    }
}
