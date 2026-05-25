// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

//! Local reconnect settings option models.

pub fn reconnect_max_attempt_options() -> [i64; 8] {
    [1, 2, 3, 5, 8, 10, 15, 20]
}

pub fn reconnect_base_delay_options() -> [(i64, &'static str); 6] {
    [
        (500, "0.5s"),
        (1_000, "1s"),
        (2_000, "2s"),
        (3_000, "3s"),
        (5_000, "5s"),
        (10_000, "10s"),
    ]
}

pub fn reconnect_max_delay_options() -> [(i64, &'static str); 5] {
    [
        (5_000, "5s"),
        (10_000, "10s"),
        (15_000, "15s"),
        (30_000, "30s"),
        (60_000, "60s"),
    ]
}

pub fn reconnect_attempt_label(value: i64) -> String {
    value.to_string()
}

pub fn reconnect_delay_label(value: i64) -> String {
    if value % 1_000 == 0 {
        format!("{}s", value / 1_000)
    } else {
        format!("{:.1}s", value as f64 / 1_000.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reconnect_delay_labels_preserve_subsecond_values() {
        assert_eq!(reconnect_delay_label(500), "0.5s");
        assert_eq!(reconnect_delay_label(2_000), "2s");
    }
}
